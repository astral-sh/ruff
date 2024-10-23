use std::fmt::Debug;
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use filetime::FileTime;
use itertools::Itertools;
use log::{debug, error};
use rayon::iter::ParallelIterator;
use rayon::iter::{IntoParallelIterator, ParallelBridge};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_diagnostics::{DiagnosticKind, Fix};
use ruff_linter::message::{DiagnosticMessage, Message};
use ruff_linter::{warn_user, VERSION};
use ruff_macros::CacheKey;
use ruff_notebook::NotebookIndex;
use ruff_source_file::SourceFileBuilder;
use ruff_text_size::{TextRange, TextSize};
use ruff_workspace::resolver::Resolver;
use ruff_workspace::Settings;

use crate::diagnostics::Diagnostics;

/// [`Path`] that is relative to the package root in [`PackageCache`].
pub(crate) type RelativePath = Path;
/// [`PathBuf`] that is relative to the package root in [`PackageCache`].
pub(crate) type RelativePathBuf = PathBuf;

#[derive(CacheKey)]
pub(crate) struct FileCacheKey {
    /// Timestamp when the file was last modified before the (cached) check.
    file_last_modified: FileTime,
    /// Permissions of the file before the (cached) check.
    file_permissions_mode: u32,
}

impl FileCacheKey {
    pub(crate) fn from_path(path: &Path) -> io::Result<FileCacheKey> {
        // Construct a cache key for the file
        let metadata = path.metadata()?;

        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode()
        };
        #[cfg(windows)]
        let permissions: u32 = metadata.permissions().readonly().into();

        Ok(FileCacheKey {
            file_last_modified: FileTime::from_last_modification_time(&metadata),
            file_permissions_mode: permissions,
        })
    }
}

/// Cache.
///
/// `Cache` holds everything required to display the diagnostics for a single
/// package. The on-disk representation is represented in [`PackageCache`] (and
/// related) types.
///
/// This type manages the cache file, reading it from disk and writing it back
/// to disk (if required).
#[derive(Debug)]
pub(crate) struct Cache {
    /// Location of the cache.
    path: PathBuf,
    /// Package cache read from disk.
    package: PackageCache,
    /// Changes made compared to the (current) `package`.
    ///
    /// Files that are linted, but are not in `package.files` or are in
    /// `package.files` but are outdated. This gets merged with `package.files`
    /// when the cache is written back to disk in [`Cache::store`].
    changes: Mutex<Vec<Change>>,
    /// The "current" timestamp used as cache for the updates of
    /// [`FileCache::last_seen`]
    #[allow(clippy::struct_field_names)]
    last_seen_cache: u64,
}

impl Cache {
    /// Open or create a new cache.
    ///
    /// `package_root` is the path to root of the package that is contained
    /// within this cache and must be canonicalized (to avoid considering `./`
    /// and `../project` being different).
    ///
    /// Finally `settings` is used to ensure we don't open a cache for different
    /// settings. It also defines the directory where to store the cache.
    pub(crate) fn open(package_root: PathBuf, settings: &Settings) -> Self {
        debug_assert!(package_root.is_absolute(), "package root not canonicalized");

        let key = format!("{}", cache_key(&package_root, settings));
        let path = PathBuf::from_iter([&settings.cache_dir, Path::new(VERSION), Path::new(&key)]);

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // No cache exist yet, return an empty cache.
                return Cache::empty(path, package_root);
            }
            Err(err) => {
                warn_user!("Failed to open cache file `{}`: {err}", path.display());
                return Cache::empty(path, package_root);
            }
        };

        let mut package: PackageCache = match bincode::deserialize_from(BufReader::new(file)) {
            Ok(package) => package,
            Err(err) => {
                warn_user!("Failed parse cache file `{}`: {err}", path.display());
                return Cache::empty(path, package_root);
            }
        };

        // Sanity check.
        if package.package_root != package_root {
            warn_user!(
                "Different package root in cache: expected `{}`, got `{}`",
                package_root.display(),
                package.package_root.display(),
            );
            package.files.clear();
        }
        Cache::new(path, package)
    }

    /// Create an empty `Cache`.
    fn empty(path: PathBuf, package_root: PathBuf) -> Self {
        let package = PackageCache {
            package_root,
            files: FxHashMap::default(),
        };
        Cache::new(path, package)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn new(path: PathBuf, package: PackageCache) -> Self {
        Cache {
            path,
            package,
            changes: Mutex::new(Vec::new()),
            // SAFETY: this will be truncated to the year ~2554 (so don't use
            // this code after that!).
            last_seen_cache: SystemTime::UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
        }
    }

    /// Applies the pending changes and persists the cache to disk, if it has been changed.
    pub(crate) fn persist(mut self) -> Result<()> {
        if !self.save() {
            // No changes made, no need to write the same cache file back to
            // disk.
            return Ok(());
        }

        // Write the cache to a temporary file first and then rename it for an "atomic" write.
        // Protects against data loss if the process is killed during the write and races between different ruff
        // processes, resulting in a corrupted cache file. https://github.com/astral-sh/ruff/issues/8147#issuecomment-1943345964
        let mut temp_file =
            NamedTempFile::new_in(self.path.parent().expect("Write path must have a parent"))
                .context("Failed to create temporary file")?;

        // Serialize to in-memory buffer because hyperfine benchmark showed that it's faster than
        // using a `BufWriter` and our cache files are small enough that streaming isn't necessary.
        let serialized =
            bincode::serialize(&self.package).context("Failed to serialize cache data")?;
        temp_file
            .write_all(&serialized)
            .context("Failed to write serialized cache to temporary file.")?;

        if let Err(err) = temp_file.persist(&self.path) {
            // On Windows, writing to the cache file can fail if the file is still open (e.g., if
            // the user is running Ruff from multiple processes over the same directory).
            if cfg!(windows) && err.error.kind() == io::ErrorKind::PermissionDenied {
                warn_user!(
                    "Failed to write cache file `{}`: {}",
                    self.path.display(),
                    err.error
                );
            } else {
                return Err(err).with_context(|| {
                    format!(
                        "Failed to rename temporary cache file to {}",
                        self.path.display()
                    )
                });
            }
        }

        Ok(())
    }

    /// Applies the pending changes without storing the cache to disk.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn save(&mut self) -> bool {
        /// Maximum duration for which we keep a file in cache that hasn't been seen.
        const MAX_LAST_SEEN: Duration = Duration::from_secs(30 * 24 * 60 * 60); // 30 days.

        let changes = std::mem::take(self.changes.get_mut().unwrap());
        if changes.is_empty() {
            return false;
        }

        // Remove cached files that we haven't seen in a while.
        let now = self.last_seen_cache;
        self.package.files.retain(|_, file| {
            // SAFETY: this will be truncated to the year ~2554.
            (now - *file.last_seen.get_mut()) <= MAX_LAST_SEEN.as_millis() as u64
        });

        // Apply any changes made and keep track of when we last saw files.
        for change in changes {
            let entry = self
                .package
                .files
                .entry(change.path)
                .and_modify(|existing| {
                    if existing.key != change.new_key {
                        // Reset the data if the key change.
                        existing.data = FileCacheData::default();
                    }

                    existing.key = change.new_key;
                    existing
                        .last_seen
                        .store(self.last_seen_cache, Ordering::Relaxed);
                })
                .or_insert_with(|| FileCache {
                    key: change.new_key,
                    last_seen: AtomicU64::new(self.last_seen_cache),
                    data: FileCacheData::default(),
                });

            change.new_data.apply(&mut entry.data);
        }

        true
    }

    /// Returns the relative path based on `path` and the package root.
    ///
    /// Returns `None` if `path` is not within the package.
    pub(crate) fn relative_path<'a>(&self, path: &'a Path) -> Option<&'a RelativePath> {
        path.strip_prefix(&self.package.package_root).ok()
    }

    /// Get the cached results for a single file at relative `path`. This
    /// uses `key` to determine if the results are still accurate.
    /// (i.e. if the file hasn't been modified since the cached run).
    ///
    /// This returns `None` if `key` differs from the cached key or if the
    /// cache doesn't contain results for the file.
    pub(crate) fn get(&self, path: &RelativePath, key: &FileCacheKey) -> Option<&FileCache> {
        let file = self.package.files.get(path)?;

        let mut hasher = CacheKeyHasher::new();
        key.cache_key(&mut hasher);

        // Make sure the file hasn't changed since the cached run.
        if file.key != hasher.finish() {
            return None;
        }

        file.last_seen.store(self.last_seen_cache, Ordering::SeqCst);

        Some(file)
    }

    pub(crate) fn is_formatted(&self, path: &RelativePath, key: &FileCacheKey) -> bool {
        self.get(path, key)
            .is_some_and(|entry| entry.data.formatted)
    }

    /// Add or update a file cache at `path` relative to the package root.
    fn update(&self, path: RelativePathBuf, key: &FileCacheKey, data: ChangeData) {
        let mut hasher = CacheKeyHasher::new();
        key.cache_key(&mut hasher);

        self.changes.lock().unwrap().push(Change {
            path,
            new_key: hasher.finish(),
            new_data: data,
        });
    }

    pub(crate) fn update_lint(
        &self,
        path: RelativePathBuf,
        key: &FileCacheKey,
        data: LintCacheData,
    ) {
        self.update(path, key, ChangeData::Lint(data));
    }

    pub(crate) fn set_formatted(&self, path: RelativePathBuf, key: &FileCacheKey) {
        self.update(path, key, ChangeData::Formatted);
    }
}

/// On disk representation of a cache of a package.
#[derive(Deserialize, Debug, Serialize)]
struct PackageCache {
    /// Path to the root of the package.
    ///
    /// Usually this is a directory, but it can also be a single file in case of
    /// single file "packages", e.g. scripts.
    package_root: PathBuf,
    /// Mapping of source file path to it's cached data.
    files: FxHashMap<RelativePathBuf, FileCache>,
}

/// On disk representation of the cache per source file.
#[derive(Deserialize, Debug, Serialize)]
pub(crate) struct FileCache {
    /// Key that determines if the cached item is still valid.
    key: u64,
    /// Timestamp when we last linted this file.
    ///
    /// Represented as the number of milliseconds since Unix epoch. This will
    /// break in 1970 + ~584 years (~2554).
    last_seen: AtomicU64,

    data: FileCacheData,
}

impl FileCache {
    /// Convert the file cache into `Diagnostics`, using `path` as file name.
    pub(crate) fn to_diagnostics(&self, path: &Path) -> Option<Diagnostics> {
        self.data.lint.as_ref().map(|lint| {
            let messages = if lint.messages.is_empty() {
                Vec::new()
            } else {
                let file = SourceFileBuilder::new(path.to_string_lossy(), &*lint.source).finish();
                lint.messages
                    .iter()
                    .map(|msg| {
                        Message::Diagnostic(DiagnosticMessage {
                            kind: msg.kind.clone(),
                            range: msg.range,
                            fix: msg.fix.clone(),
                            file: file.clone(),
                            noqa_offset: msg.noqa_offset,
                        })
                    })
                    .collect()
            };
            let notebook_indexes = if let Some(notebook_index) = lint.notebook_index.as_ref() {
                FxHashMap::from_iter([(path.to_string_lossy().to_string(), notebook_index.clone())])
            } else {
                FxHashMap::default()
            };
            Diagnostics::new(messages, notebook_indexes)
        })
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct FileCacheData {
    lint: Option<LintCacheData>,
    formatted: bool,
}

/// Returns a hash key based on the `package_root`, `settings` and the crate
/// version.
fn cache_key(package_root: &Path, settings: &Settings) -> u64 {
    let mut hasher = CacheKeyHasher::new();
    package_root.cache_key(&mut hasher);
    settings.cache_key(&mut hasher);

    hasher.finish()
}

/// Initialize the cache at the specified `Path`.
pub(crate) fn init(path: &Path) -> Result<()> {
    // Create the cache directories.
    fs::create_dir_all(path.join(VERSION))?;

    // Add the CACHEDIR.TAG.
    cachedir::ensure_tag(path)?;

    // Add the .gitignore.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.join(".gitignore"))
    {
        Ok(mut file) => file.write_all(b"# Automatically created by ruff.\n*\n")?,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
        Err(err) => return Err(err.into()),
    }

    Ok(())
}

#[derive(Deserialize, Debug, Serialize, PartialEq)]
pub(crate) struct LintCacheData {
    /// Imports made.
    // pub(super) imports: ImportMap,
    /// Diagnostic messages.
    pub(super) messages: Vec<CacheMessage>,
    /// Source code of the file.
    ///
    /// # Notes
    ///
    /// This will be empty if `messages` is empty.
    pub(super) source: String,
    /// Notebook index if this file is a Jupyter Notebook.
    pub(super) notebook_index: Option<NotebookIndex>,
}

impl LintCacheData {
    pub(crate) fn from_messages(
        messages: &[Message],
        notebook_index: Option<NotebookIndex>,
    ) -> Self {
        let source = if let Some(msg) = messages.first() {
            msg.source_file().source_text().to_owned()
        } else {
            String::new() // No messages, no need to keep the source!
        };

        let messages = messages
            .iter()
            .filter_map(|message| message.as_diagnostic_message())
            .map(|msg| {
                // Make sure that all message use the same source file.
                assert_eq!(
                    &msg.file,
                    messages.first().unwrap().source_file(),
                    "message uses a different source file"
                );
                CacheMessage {
                    kind: msg.kind.clone(),
                    range: msg.range,
                    fix: msg.fix.clone(),
                    noqa_offset: msg.noqa_offset,
                }
            })
            .collect();

        Self {
            messages,
            source,
            notebook_index,
        }
    }
}

/// On disk representation of a diagnostic message.
#[derive(Deserialize, Debug, Serialize, PartialEq)]
pub(super) struct CacheMessage {
    kind: DiagnosticKind,
    /// Range into the message's [`FileCache::source`].
    range: TextRange,
    fix: Option<Fix>,
    noqa_offset: TextSize,
}

pub(crate) trait PackageCaches {
    fn get(&self, package_root: &Path) -> Option<&Cache>;

    fn persist(self) -> Result<()>;
}

impl<T> PackageCaches for Option<T>
where
    T: PackageCaches,
{
    fn get(&self, package_root: &Path) -> Option<&Cache> {
        match self {
            None => None,
            Some(caches) => caches.get(package_root),
        }
    }

    fn persist(self) -> Result<()> {
        match self {
            None => Ok(()),
            Some(caches) => caches.persist(),
        }
    }
}

pub(crate) struct PackageCacheMap<'a>(FxHashMap<&'a Path, Cache>);

impl<'a> PackageCacheMap<'a> {
    pub(crate) fn init(
        package_roots: &FxHashMap<&'a Path, Option<&'a Path>>,
        resolver: &Resolver,
    ) -> Self {
        fn init_cache(path: &Path) {
            if let Err(e) = init(path) {
                error!("Failed to initialize cache at {}: {e:?}", path.display());
            }
        }

        for settings in resolver.settings() {
            init_cache(&settings.cache_dir);
        }

        Self(
            package_roots
                .iter()
                .map(|(package, package_root)| package_root.unwrap_or(package))
                .unique()
                .par_bridge()
                .map(|cache_root| {
                    let settings = resolver.resolve(cache_root);
                    let cache = Cache::open(cache_root.to_path_buf(), settings);
                    (cache_root, cache)
                })
                .collect(),
        )
    }
}

impl PackageCaches for PackageCacheMap<'_> {
    fn get(&self, package_root: &Path) -> Option<&Cache> {
        let cache = self.0.get(package_root);

        if cache.is_none() {
            debug!("No cache found for {}", package_root.display());
        }

        cache
    }

    fn persist(self) -> Result<()> {
        self.0
            .into_par_iter()
            .try_for_each(|(_, cache)| cache.persist())
    }
}

#[derive(Debug)]
struct Change {
    path: PathBuf,
    new_key: u64,
    new_data: ChangeData,
}

#[derive(Debug)]
enum ChangeData {
    Lint(LintCacheData),
    Formatted,
}

impl ChangeData {
    fn apply(self, data: &mut FileCacheData) {
        match self {
            ChangeData::Lint(new_lint) => {
                data.lint = Some(new_lint);
            }
            ChangeData::Formatted => {
                data.formatted = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs;
    use std::io;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicU64;
    use std::time::SystemTime;

    use anyhow::Result;
    use filetime::{set_file_mtime, FileTime};
    use itertools::Itertools;
    use test_case::test_case;

    use ruff_cache::CACHE_DIR_NAME;
    use ruff_linter::message::Message;
    use ruff_linter::settings::flags;
    use ruff_linter::settings::types::UnsafeFixes;
    use ruff_python_ast::PySourceType;
    use ruff_workspace::Settings;

    use crate::cache::{self, FileCache, FileCacheData, FileCacheKey};
    use crate::cache::{Cache, RelativePathBuf};
    use crate::commands::format::{format_path, FormatCommandError, FormatMode, FormatResult};
    use crate::diagnostics::{lint_path, Diagnostics};

    #[test_case("../ruff_linter/resources/test/fixtures", "ruff_tests/cache_same_results_ruff_linter"; "ruff_linter_fixtures")]
    #[test_case("../ruff_notebook/resources/test/fixtures", "ruff_tests/cache_same_results_ruff_notebook"; "ruff_notebook_fixtures")]
    fn same_results(package_root: &str, cache_dir_path: &str) {
        let mut cache_dir = temp_dir();
        cache_dir.push(cache_dir_path);
        let _ = fs::remove_dir_all(&cache_dir);
        cache::init(&cache_dir).unwrap();

        let settings = Settings {
            cache_dir,
            ..Settings::default()
        };

        let package_root = fs::canonicalize(package_root).unwrap();
        let cache = Cache::open(package_root.clone(), &settings);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        let mut paths = Vec::new();
        let mut parse_errors = Vec::new();
        let mut expected_diagnostics = Diagnostics::default();
        for entry in fs::read_dir(&package_root).unwrap() {
            let entry = entry.unwrap();
            if !entry.file_type().unwrap().is_dir() {
                continue;
            }

            let dir_path = entry.path();
            if dir_path.ends_with(CACHE_DIR_NAME) {
                continue;
            }

            for entry in fs::read_dir(dir_path).unwrap() {
                let entry = entry.unwrap();
                if !entry.file_type().unwrap().is_file() {
                    continue;
                }

                let path = entry.path();
                if path.ends_with("pyproject.toml") || path.ends_with("R.ipynb") {
                    continue;
                }

                let diagnostics = lint_path(
                    &path,
                    Some(&package_root),
                    &settings.linter,
                    Some(&cache),
                    flags::Noqa::Enabled,
                    flags::FixMode::Generate,
                    UnsafeFixes::Enabled,
                )
                .unwrap();
                if diagnostics.messages.iter().any(Message::is_syntax_error) {
                    parse_errors.push(path.clone());
                }
                paths.push(path);
                expected_diagnostics += diagnostics;
            }
        }
        assert_ne!(paths, &[] as &[std::path::PathBuf], "no files checked");

        cache.persist().unwrap();

        let cache = Cache::open(package_root.clone(), &settings);
        assert_ne!(cache.package.files.len(), 0);

        parse_errors.sort();

        for path in &paths {
            if parse_errors.binary_search(path).is_ok() {
                continue; // We don't cache parsing errors.
            }

            let relative_path = cache.relative_path(path).unwrap();

            assert!(
                cache.package.files.contains_key(relative_path),
                "missing file from cache: `{}`",
                relative_path.display()
            );
        }

        let mut got_diagnostics = Diagnostics::default();
        for path in paths {
            got_diagnostics += lint_path(
                &path,
                Some(&package_root),
                &settings.linter,
                Some(&cache),
                flags::Noqa::Enabled,
                flags::FixMode::Generate,
                UnsafeFixes::Enabled,
            )
            .unwrap();
        }

        assert_eq!(expected_diagnostics, got_diagnostics);
    }

    #[test]
    fn cache_adds_file_on_lint() {
        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let test_cache = TestCache::new("cache_adds_file_on_lint");
        let cache = test_cache.open();
        test_cache.write_source_file("source.py", source);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        cache.persist().unwrap();
        let cache = test_cache.open();

        test_cache
            .lint_file_with_cache("source.py", &cache)
            .expect("Failed to lint test file");
        assert_eq!(
            cache.changes.lock().unwrap().len(),
            1,
            "A single new file should be added to the cache"
        );

        cache.persist().unwrap();
    }

    #[test]
    fn cache_adds_files_on_lint() {
        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let test_cache = TestCache::new("cache_adds_files_on_lint");
        let cache = test_cache.open();
        test_cache.write_source_file("source_1.py", source);
        test_cache.write_source_file("source_2.py", source);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        cache.persist().unwrap();
        let cache = test_cache.open();

        test_cache
            .lint_file_with_cache("source_1.py", &cache)
            .expect("Failed to lint test file");
        test_cache
            .lint_file_with_cache("source_2.py", &cache)
            .expect("Failed to lint test file");
        assert_eq!(
            cache.changes.lock().unwrap().len(),
            2,
            "Both files should be added to the cache"
        );
        cache.persist().unwrap();
    }

    #[test]
    fn cache_adds_files_on_format() {
        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let test_cache = TestCache::new("cache_adds_files_on_format");
        let cache = test_cache.open();
        test_cache.write_source_file("source_1.py", source);
        test_cache.write_source_file("source_2.py", source);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        cache.persist().unwrap();
        let cache = test_cache.open();

        test_cache
            .format_file_with_cache("source_1.py", &cache)
            .expect("Failed to format test file");
        test_cache
            .format_file_with_cache("source_2.py", &cache)
            .expect("Failed to format test file");
        assert_eq!(
            cache.changes.lock().unwrap().len(),
            2,
            "Both files should be added to the cache"
        );

        cache.persist().unwrap();
    }

    #[test]
    fn cache_invalidated_on_file_modified_time() {
        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let test_cache = TestCache::new("cache_invalidated_on_file_modified_time");
        let cache = test_cache.open();
        let source_path = test_cache.write_source_file("source.py", source);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        let expected_diagnostics = test_cache
            .lint_file_with_cache("source.py", &cache)
            .expect("Failed to lint test file");

        cache.persist().unwrap();
        let cache = test_cache.open();

        // Update the modified time of the file to a time in the future
        set_file_mtime(
            source_path,
            FileTime::from_system_time(SystemTime::now() + std::time::Duration::from_secs(1)),
        )
        .unwrap();

        let got_diagnostics = test_cache
            .lint_file_with_cache("source.py", &cache)
            .expect("Failed to lint test file");

        assert_eq!(
            cache.changes.lock().unwrap().len(),
            1,
            "Cache should not be used, the file should be treated as new and added to the cache"
        );

        assert_eq!(
            expected_diagnostics, got_diagnostics,
            "The diagnostics should not change"
        );
    }

    #[test]
    fn cache_invalidated_on_permission_change() {
        // Regression test for issue #3086.

        #[cfg(unix)]
        #[allow(clippy::items_after_statements)]
        fn flip_execute_permission_bit(path: &Path) -> io::Result<()> {
            use std::os::unix::fs::PermissionsExt;
            let file = fs::OpenOptions::new().write(true).open(path)?;
            let perms = file.metadata()?.permissions();
            file.set_permissions(PermissionsExt::from_mode(perms.mode() ^ 0o111))
        }

        #[cfg(windows)]
        #[allow(clippy::items_after_statements)]
        fn flip_read_only_permission(path: &Path) -> io::Result<()> {
            let file = fs::OpenOptions::new().write(true).open(path)?;
            let mut perms = file.metadata()?.permissions();
            perms.set_readonly(!perms.readonly());
            file.set_permissions(perms)
        }

        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let test_cache = TestCache::new("cache_invalidated_on_permission_change");
        let cache = test_cache.open();
        let path = test_cache.write_source_file("source.py", source);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        let expected_diagnostics = test_cache
            .lint_file_with_cache("source.py", &cache)
            .unwrap();

        cache.persist().unwrap();
        let cache = test_cache.open();

        // Flip the permissions on the file
        #[cfg(unix)]
        flip_execute_permission_bit(&path).unwrap();
        #[cfg(windows)]
        flip_read_only_permission(&path).unwrap();

        let got_diagnostics = test_cache
            .lint_file_with_cache("source.py", &cache)
            .unwrap();

        assert_eq!(
            cache.changes.lock().unwrap().len(),
            1,
            "Cache should not be used, the file should be treated as new and added to the cache"
        );

        assert_eq!(
            expected_diagnostics, got_diagnostics,
            "The diagnostics should not change"
        );
    }

    #[test]
    fn cache_removes_stale_files_on_persist() {
        let test_cache = TestCache::new("cache_removes_stale_files_on_persist");
        let mut cache = test_cache.open();

        // Add a file to the cache that hasn't been linted or seen since the '70s!
        let old_path_key = RelativePathBuf::from("old.py");
        cache.package.files.insert(
            old_path_key,
            FileCache {
                key: 123,
                last_seen: AtomicU64::new(123),
                data: FileCacheData::default(),
            },
        );

        // Now actually lint a file.
        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";
        test_cache.write_source_file("new.py", source);
        let new_path_key = RelativePathBuf::from("new.py");
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        test_cache
            .lint_file_with_cache("new.py", &cache)
            .expect("Failed to lint test file");

        // Storing the cache should remove the old (`old.py`) file.
        cache.persist().unwrap();
        // So we when we open the cache again it shouldn't contain `old.py`.
        let cache = test_cache.open();

        assert_eq!(
            cache.package.files.keys().collect_vec(),
            vec![&new_path_key],
            "Only the new file should be present"
        );
    }

    #[test]
    fn format_updates_cache_entry() {
        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let test_cache = TestCache::new("format_updates_cache_entry");
        let cache = test_cache.open();
        test_cache.write_source_file("source.py", source);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        cache.persist().unwrap();
        let cache = test_cache.open();

        // Cache the lint results
        test_cache
            .lint_file_with_cache("source.py", &cache)
            .expect("Failed to lint test file");
        cache.persist().unwrap();

        let mut cache = test_cache.open();

        // Now lint the file
        test_cache
            .format_file_with_cache("source.py", &cache)
            .expect("Failed to format test file");

        cache.save();

        assert_eq!(cache.package.files.len(), 1);

        let Some(file_cache) = cache.get(
            Path::new("source.py"),
            &FileCacheKey::from_path(&test_cache.package_root.join("source.py")).unwrap(),
        ) else {
            panic!("Cache entry for `source.py` is missing.");
        };

        assert!(file_cache.data.lint.is_some());
        assert!(file_cache.data.formatted);
    }

    #[test]
    fn file_changes_invalidate_file_cache() {
        let source: &[u8] = b"a = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let test_cache = TestCache::new("file_changes_invalidate_file_cache");
        let cache = test_cache.open();
        let source_path = test_cache.write_source_file("source.py", source);
        assert_eq!(cache.changes.lock().unwrap().len(), 0);

        cache.persist().unwrap();
        let cache = test_cache.open();

        // Cache the format and lint results
        test_cache
            .lint_file_with_cache("source.py", &cache)
            .expect("Failed to lint test file");
        test_cache
            .format_file_with_cache("source.py", &cache)
            .expect("Failed to format test file");

        cache.persist().unwrap();

        let mut cache = test_cache.open();
        assert_eq!(cache.package.files.len(), 1);

        set_file_mtime(
            &source_path,
            FileTime::from_system_time(SystemTime::now() + std::time::Duration::from_secs(1)),
        )
        .unwrap();

        test_cache
            .format_file_with_cache("source.py", &cache)
            .expect("Failed to format test file");

        cache.save();

        assert_eq!(cache.package.files.len(), 1);

        let Some(file_cache) = cache.get(
            Path::new("source.py"),
            &FileCacheKey::from_path(&source_path).unwrap(),
        ) else {
            panic!("Cache entry for `source.py` is missing.");
        };

        assert_eq!(file_cache.data.lint, None);
        assert!(file_cache.data.formatted);
    }

    struct TestCache {
        package_root: PathBuf,
        settings: Settings,
    }

    impl TestCache {
        fn new(test_case: &str) -> Self {
            // Build a new cache directory and clear it
            let mut test_dir = temp_dir();
            test_dir.push("ruff_tests/cache");
            test_dir.push(test_case);

            let _ = fs::remove_dir_all(&test_dir);

            // Create separate directories for the cache and the test package
            let cache_dir = test_dir.join("cache");
            let package_root = test_dir.join("package");

            cache::init(&cache_dir).unwrap();
            fs::create_dir(package_root.clone()).unwrap();

            let settings = Settings {
                cache_dir,
                ..Settings::default()
            };

            Self {
                package_root,
                settings,
            }
        }

        fn write_source_file(&self, path: &str, contents: &[u8]) -> PathBuf {
            let path = self.package_root.join(path);
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&*path)
                .unwrap();
            file.write_all(contents).unwrap();
            file.sync_data().unwrap();
            path
        }

        fn open(&self) -> Cache {
            Cache::open(self.package_root.clone(), &self.settings)
        }

        fn lint_file_with_cache(
            &self,
            path: &str,
            cache: &Cache,
        ) -> Result<Diagnostics, anyhow::Error> {
            lint_path(
                &self.package_root.join(path),
                Some(&self.package_root),
                &self.settings.linter,
                Some(cache),
                flags::Noqa::Enabled,
                flags::FixMode::Generate,
                UnsafeFixes::Enabled,
            )
        }

        fn format_file_with_cache(
            &self,
            path: &str,
            cache: &Cache,
        ) -> Result<FormatResult, FormatCommandError> {
            let file_path = self.package_root.join(path);
            format_path(
                &file_path,
                &self.settings.formatter,
                PySourceType::Python,
                FormatMode::Write,
                None,
                Some(cache),
            )
        }
    }

    impl Drop for TestCache {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.settings.cache_dir);
        }
    }
}
