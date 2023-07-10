use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use ruff::message::Message;
use ruff::settings::Settings;
use ruff::warn_user;
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_diagnostics::{DiagnosticKind, Fix};
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::source_code::SourceFileBuilder;
use ruff_text_size::{TextRange, TextSize};

use crate::diagnostics::Diagnostics;

/// Maximum duration for which we keep a file in cache that hasn't been seen.
const MAX_LAST_SEEN: Duration = Duration::from_secs(30 * 24 * 60 * 60); // 30 days.

/// [`Path`] that is relative to the package root in [`PackageCache`].
pub(crate) type RelativePath = Path;
/// [`PathBuf`] that is relative to the package root in [`PackageCache`].
pub(crate) type RelativePathBuf = PathBuf;

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
    new_files: Mutex<HashMap<RelativePathBuf, FileCache>>,
    /// The "current" timestamp used as cache for the updates of
    /// [`FileCache::last_seen`]
    last_seen_cache: u64,
}

impl Cache {
    /// Open or create a new cache.
    ///
    /// `cache_dir` is considered the root directory of the cache, which can be
    /// local to the project, global or otherwise set by the user.
    ///
    /// `package_root` is the path to root of the package that is contained
    /// within this cache and must be canonicalized (to avoid considering `./`
    /// and `../project` being different).
    ///
    /// Finally `settings` is used to ensure we don't open a cache for different
    /// settings.
    pub(crate) fn open(cache_dir: &Path, package_root: PathBuf, settings: &Settings) -> Cache {
        debug_assert!(package_root.is_absolute(), "package root not canonicalized");

        let mut buf = itoa::Buffer::new();
        let key = Path::new(buf.format(cache_key(&package_root, settings)));
        let path = PathBuf::from_iter([cache_dir, Path::new("content"), key]);

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // No cache exist yet, return an empty cache.
                return Cache::empty(path, package_root);
            }
            Err(err) => {
                warn_user!("Failed to open cache file '{}': {err}", path.display());
                return Cache::empty(path, package_root);
            }
        };

        let mut package: PackageCache = match bincode::deserialize_from(BufReader::new(file)) {
            Ok(package) => package,
            Err(err) => {
                warn_user!("Failed parse cache file '{}': {err}", path.display());
                return Cache::empty(path, package_root);
            }
        };

        // Sanity check.
        if package.package_root != package_root {
            warn_user!(
                "Different package root in cache: expected '{}', got '{}'",
                package_root.display(),
                package.package_root.display(),
            );
            package.files.clear();
        }
        Cache::new(path, package)
    }

    /// Create an empty `Cache`.
    fn empty(path: PathBuf, package_root: PathBuf) -> Cache {
        let package = PackageCache {
            package_root,
            files: HashMap::new(),
        };
        Cache::new(path, package)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn new(path: PathBuf, package: PackageCache) -> Cache {
        Cache {
            path,
            package,
            new_files: Mutex::new(HashMap::new()),
            // SAFETY: this will be truncated to the year ~2554 (so don't use
            // this code after that!).
            last_seen_cache: SystemTime::UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
        }
    }

    /// Store the cache to disk, if it has been changed.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn store(mut self) -> Result<()> {
        let new_files = self.new_files.into_inner().unwrap();
        if new_files.is_empty() {
            // No changes made, no need to write the same cache file back to
            // disk.
            return Ok(());
        }

        // Remove cached files that we haven't seen in a while.
        let now = self.last_seen_cache;
        self.package.files.retain(|_, file| {
            // SAFETY: this will be truncated to the year ~2554.
            (now - *file.last_seen.get_mut()) <= MAX_LAST_SEEN.as_millis() as u64
        });

        // Apply any changes made and keep track of when we last saw files.
        self.package.files.extend(new_files);

        let file = File::create(&self.path)
            .with_context(|| format!("Failed to create cache file '{}'", self.path.display()))?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, &self.package).with_context(|| {
            format!(
                "Failed to serialise cache to file '{}'",
                self.path.display()
            )
        })
    }

    /// Returns the relative path based on `path` and the package root.
    ///
    /// Returns `None` if `path` is not within the package.
    pub(crate) fn relative_path<'a>(&self, path: &'a Path) -> Option<&'a RelativePath> {
        path.strip_prefix(&self.package.package_root).ok()
    }

    /// Get the cached results for a single file at relative `path`. This uses
    /// `file_last_modified` to determine if the results are still accurate
    /// (i.e. if the file hasn't been modified since the cached run).
    ///
    /// This returns `None` if `file_last_modified` differs from the cached
    /// timestamp or if the cache doesn't contain results for the file.
    pub(crate) fn get(
        &self,
        path: &RelativePath,
        file_last_modified: SystemTime,
    ) -> Option<&FileCache> {
        let file = self.package.files.get(path)?;

        // Make sure the file hasn't changed since the cached run.
        if file.last_modified != file_last_modified {
            return None;
        }

        file.last_seen.store(self.last_seen_cache, Ordering::SeqCst);

        Some(file)
    }

    /// Add or update a file cache at `path` relative to the package root.
    pub(crate) fn update(
        &self,
        path: RelativePathBuf,
        last_modified: SystemTime,
        messages: &[Message],
        imports: &ImportMap,
    ) {
        let source = if let Some(msg) = messages.first() {
            msg.file.source_text().to_owned()
        } else {
            String::new() // No messages, no need to keep the source!
        };

        let messages = messages
            .iter()
            .map(|msg| {
                // Make sure that all message use the same source file.
                assert!(
                    msg.file == messages.first().unwrap().file,
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

        let file = FileCache {
            last_modified,
            last_seen: AtomicU64::new(self.last_seen_cache),
            imports: imports.clone(),
            messages,
            source,
        };
        self.new_files.lock().unwrap().insert(path, file);
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
    files: HashMap<RelativePathBuf, FileCache>,
}

/// On disk representation of the cache per source file.
#[derive(Deserialize, Debug, Serialize)]
pub(crate) struct FileCache {
    /// Timestamp when the file was last modified before the (cached) check.
    last_modified: SystemTime,
    /// Timestamp when we last linted this file.
    ///
    /// Represented as the number of milliseconds since Unix epoch. This will
    /// break in 1970 + ~584 years (~2554).
    last_seen: AtomicU64,
    /// Imports made.
    imports: ImportMap,
    /// Diagnostic messages.
    messages: Vec<CacheMessage>,
    /// Source code of the file.
    ///
    /// # Notes
    ///
    /// This will be empty if `messages` is empty.
    source: String,
}

impl FileCache {
    /// Convert the file cache into `Diagnostics`, using `path` as file name.
    pub(crate) fn as_diagnostics(&self, path: &Path) -> Diagnostics {
        let messages = if self.messages.is_empty() {
            Vec::new()
        } else {
            let file = SourceFileBuilder::new(path.to_string_lossy(), &*self.source).finish();
            self.messages
                .iter()
                .map(|msg| Message {
                    kind: msg.kind.clone(),
                    range: msg.range,
                    fix: msg.fix.clone(),
                    file: file.clone(),
                    noqa_offset: msg.noqa_offset,
                })
                .collect()
        };
        Diagnostics::new(messages, self.imports.clone())
    }
}

/// On disk representation of a diagnostic message.
#[derive(Deserialize, Debug, Serialize)]
struct CacheMessage {
    kind: DiagnosticKind,
    /// Range into the message's [`FileCache::source`].
    range: TextRange,
    fix: Option<Fix>,
    noqa_offset: TextSize,
}

/// Returns a hash key based on the `package_root`, `settings` and the crate
/// version.
fn cache_key(package_root: &Path, settings: &Settings) -> u64 {
    let mut hasher = CacheKeyHasher::new();
    env!("CARGO_PKG_VERSION").cache_key(&mut hasher);
    package_root.cache_key(&mut hasher);
    settings.cache_key(&mut hasher);
    hasher.finish()
}

/// Initialize the cache at the specified `Path`.
pub(crate) fn init(path: &Path) -> Result<()> {
    // Create the cache directories.
    fs::create_dir_all(path.join("content"))?;

    // Add the CACHEDIR.TAG.
    if !cachedir::is_tagged(path)? {
        cachedir::add_tag(path)?;
    }

    // Add the .gitignore.
    let gitignore_path = path.join(".gitignore");
    if !gitignore_path.exists() {
        let mut file = fs::File::create(gitignore_path)?;
        file.write_all(b"*")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::fs;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicU64;
    use std::time::SystemTime;

    use ruff::settings::{flags, AllSettings};
    use ruff_cache::CACHE_DIR_NAME;
    use ruff_python_ast::imports::ImportMap;

    use crate::cache::{self, Cache, FileCache};
    use crate::diagnostics::{lint_path, Diagnostics};

    #[test]
    fn same_results() {
        let mut cache_dir = temp_dir();
        cache_dir.push("ruff_tests/cache_same_results");
        let _ = fs::remove_dir_all(&cache_dir);
        cache::init(&cache_dir).unwrap();

        let settings = AllSettings::default();

        let package_root = fs::canonicalize("../ruff/resources/test/fixtures").unwrap();
        let cache = Cache::open(&cache_dir, package_root.clone(), &settings.lib);
        assert_eq!(cache.new_files.lock().unwrap().len(), 0);

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
                    &settings,
                    Some(&cache),
                    flags::Noqa::Enabled,
                    flags::FixMode::Generate,
                )
                .unwrap();
                if diagnostics
                    .messages
                    .iter()
                    .any(|m| m.kind.name == "SyntaxError")
                {
                    parse_errors.push(path.clone());
                }
                paths.push(path);
                expected_diagnostics += diagnostics;
            }
        }
        assert_ne!(paths, &[] as &[std::path::PathBuf], "no files checked");

        cache.store().unwrap();

        let cache = Cache::open(&cache_dir, package_root.clone(), &settings.lib);
        assert_ne!(cache.package.files.len(), 0);

        parse_errors.sort();

        for path in &paths {
            if parse_errors.binary_search(path).is_ok() {
                continue; // We don't cache parsing errors.
            }

            let relative_path = cache.relative_path(path).unwrap();

            assert!(
                cache.package.files.contains_key(relative_path),
                "missing file from cache: '{}'",
                relative_path.display()
            );
        }

        let mut got_diagnostics = Diagnostics::default();
        for path in paths {
            got_diagnostics += lint_path(
                &path,
                Some(&package_root),
                &settings,
                Some(&cache),
                flags::Noqa::Enabled,
                flags::FixMode::Generate,
            )
            .unwrap();
        }

        // Not stored in the cache.
        expected_diagnostics.source_kind.clear();
        got_diagnostics.source_kind.clear();
        assert!(expected_diagnostics == got_diagnostics);
    }

    #[test]
    fn invalidation() {
        // NOTE: keep in sync with actual file.
        const SOURCE: &[u8] = b"# NOTE: sync with cache::invalidation test\na = 1\n\n__all__ = list([\"a\", \"b\"])\n";

        let mut cache_dir = temp_dir();
        cache_dir.push("ruff_tests/cache_invalidation");
        let _ = fs::remove_dir_all(&cache_dir);
        cache::init(&cache_dir).unwrap();

        let settings = AllSettings::default();
        let package_root = fs::canonicalize("resources/test/fixtures/cache_mutable").unwrap();
        let cache = Cache::open(&cache_dir, package_root.clone(), &settings.lib);
        assert_eq!(cache.new_files.lock().unwrap().len(), 0);

        let path = package_root.join("source.py");
        let mut expected_diagnostics = lint_path(
            &path,
            Some(&package_root),
            &settings,
            Some(&cache),
            flags::Noqa::Enabled,
            flags::FixMode::Generate,
        )
        .unwrap();
        assert_eq!(cache.new_files.lock().unwrap().len(), 1);

        cache.store().unwrap();

        let tests = [
            // File change.
            (|path| {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(path)?;
                file.write_all(SOURCE)?;
                file.sync_data()?;
                Ok(|_| Ok(()))
            }) as fn(&Path) -> io::Result<fn(&Path) -> io::Result<()>>,
            // Regression for issue #3086.
            #[cfg(unix)]
            |path| {
                flip_execute_permission_bit(path)?;
                Ok(flip_execute_permission_bit)
            },
        ];

        #[cfg(unix)]
        #[allow(clippy::items_after_statements)]
        fn flip_execute_permission_bit(path: &Path) -> io::Result<()> {
            use std::os::unix::fs::PermissionsExt;
            let file = fs::OpenOptions::new().write(true).open(path)?;
            let perms = file.metadata()?.permissions();
            file.set_permissions(PermissionsExt::from_mode(perms.mode() ^ 0o111))
        }

        for change_file in tests {
            let cleanup = change_file(&path).unwrap();

            let cache = Cache::open(&cache_dir, package_root.clone(), &settings.lib);

            let mut got_diagnostics = lint_path(
                &path,
                Some(&package_root),
                &settings,
                Some(&cache),
                flags::Noqa::Enabled,
                flags::FixMode::Generate,
            )
            .unwrap();

            cleanup(&path).unwrap();

            assert_eq!(
                cache.new_files.lock().unwrap().len(),
                1,
                "cache must not be used"
            );

            // Not store in the cache.
            expected_diagnostics.source_kind.clear();
            got_diagnostics.source_kind.clear();
            assert!(expected_diagnostics == got_diagnostics);
        }
    }

    #[test]
    fn remove_old_files() {
        let mut cache_dir = temp_dir();
        cache_dir.push("ruff_tests/cache_remove_old_files");
        let _ = fs::remove_dir_all(&cache_dir);
        cache::init(&cache_dir).unwrap();

        let settings = AllSettings::default();
        let package_root =
            fs::canonicalize("resources/test/fixtures/cache_remove_old_files").unwrap();
        let mut cache = Cache::open(&cache_dir, package_root.clone(), &settings.lib);
        assert_eq!(cache.new_files.lock().unwrap().len(), 0);

        // Add a file to the cache that hasn't been linted or seen since the
        // '70s!
        cache.package.files.insert(
            PathBuf::from("old.py"),
            FileCache {
                last_modified: SystemTime::UNIX_EPOCH,
                last_seen: AtomicU64::new(123),
                imports: ImportMap::new(),
                messages: Vec::new(),
                source: String::new(),
            },
        );

        // Now actually lint a file.
        let path = package_root.join("source.py");
        lint_path(
            &path,
            Some(&package_root),
            &settings,
            Some(&cache),
            flags::Noqa::Enabled,
            flags::FixMode::Generate,
        )
        .unwrap();

        // Storing the cache should remove the old (`old.py`) file.
        cache.store().unwrap();
        // So we when we open the cache again it shouldn't contain `old.py`.
        let cache = Cache::open(&cache_dir, package_root, &settings.lib);

        assert_eq!(cache.package.files.len(), 1, "didn't remove the old file");
        assert!(
            !cache.package.files.contains_key(&path),
            "removed the wrong file"
        );
    }
}
