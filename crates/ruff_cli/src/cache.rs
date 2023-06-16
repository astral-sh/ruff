use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

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

/// On disk representation of a cache of a package.
#[derive(Deserialize, Debug, Serialize)]
pub(crate) struct PackageCache {
    /// Location of the cache.
    ///
    /// Not stored on disk, just used as a storage location.
    #[serde(skip)]
    path: PathBuf,
    /// Boolean indicating the cache has been changed, used by
    /// [`PackageCache::store`] to determine if we need to write the change back
    /// to disk or not.
    ///
    /// Not stored on disk.
    #[serde(skip)]
    changed: AtomicBool,
    /// Path to the root of the package.
    ///
    /// Usually this is a directory, but it can also be a single file in case of
    /// single file "packages", e.g. scripts.
    package_root: PathBuf,
    /// Mapping of source file path to it's cached data.
    // TODO: look into concurrent hashmap or similar instead of a mutex.
    files: Mutex<HashMap<RelativePathBuf, FileCache>>,
}

impl PackageCache {
    /// Open or create a new package cache.
    ///
    /// `package_root` must be canonicalized.
    pub(crate) fn open(
        cache_dir: &Path,
        package_root: PathBuf,
        settings: &Settings,
    ) -> PackageCache {
        debug_assert!(package_root.is_absolute(), "package root not canonicalized");

        let mut buf = itoa::Buffer::new();
        let key = Path::new(buf.format(cache_key(&package_root, settings)));
        let path = PathBuf::from_iter([cache_dir, Path::new("content"), key]);

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // No cache exist yet, return an empty cache.
                return PackageCache::empty(path, package_root);
            }
            Err(err) => {
                warn_user!("Failed to open cache file '{}': {err}", path.display());
                return PackageCache::empty(path, package_root);
            }
        };

        let mut cache: PackageCache = match bincode::deserialize_from(BufReader::new(file)) {
            Ok(cache) => cache,
            Err(err) => {
                warn_user!("Failed parse cache file '{}': {err}", path.display());
                return PackageCache::empty(path, package_root);
            }
        };

        // Sanity check.
        if cache.package_root != package_root {
            warn_user!(
                "Different package root in cache: expected '{}', got '{}'",
                package_root.display(),
                cache.package_root.display(),
            );
            cache.files.lock().unwrap().clear();
        }

        cache.path = path;
        cache
    }

    pub(crate) fn empty(path: PathBuf, package_root: PathBuf) -> PackageCache {
        PackageCache {
            path,
            changed: AtomicBool::new(false),
            package_root,
            files: Mutex::new(HashMap::new()),
        }
    }

    /// Store the cache to disk.
    ///
    /// # Notes
    ///
    /// This does not write the cache back to disk if it has not been changed.
    pub(crate) fn store(&self) -> Result<()> {
        if !self.changed.load(Ordering::Acquire) {
            // No changes made, no need to write the same cache file back to
            // disk.
            return Ok(());
        }

        let file = File::create(&self.path)
            .with_context(|| format!("Failed to create cache file '{}'", self.path.display()))?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, &self).with_context(|| {
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
        path.strip_prefix(&self.package_root).ok()
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
    ) -> Option<FileCache> {
        let files = self.files.lock().unwrap();
        let file = files.get(path)?;

        // Make sure the file hasn't changed since the cached run.
        if file.last_modified != file_last_modified {
            return None;
        }

        Some(file.clone())
    }

    /// Add or update a file cache at `path` relative to the package root.
    pub(crate) fn update(&self, path: RelativePathBuf, file: FileCache) {
        self.set_changed();
        self.files.lock().unwrap().insert(path, file);
    }

    /// Remove a file cache at `path` relative to the package root.
    pub(crate) fn remove(&self, path: &RelativePath) {
        self.set_changed();
        self.files.lock().unwrap().remove(path);
    }

    /// Mark the cache as changed, indicting we need to write it to disk again.
    fn set_changed(&self) {
        if !self.changed.load(Ordering::Relaxed) {
            self.changed.store(true, Ordering::Release);
        }
    }
}

/// [`Path`] that is relative to the package root in [`PackageCache`].
pub(crate) type RelativePath = Path;
/// [`PathBuf`] that is relative to the package root in [`PackageCache`].
pub(crate) type RelativePathBuf = PathBuf;

/// On disk representation of the cache per source file.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub(crate) struct FileCache {
    /// Timestamp when the file was last modified before the (cached) check.
    last_modified: SystemTime,
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
    /// Create a new source file cache.
    pub(crate) fn new(
        last_modified: SystemTime,
        messages: &[Message],
        imports: &ImportMap,
    ) -> FileCache {
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

        FileCache {
            last_modified,
            imports: imports.clone(),
            messages,
            source,
        }
    }

    /// Convert the file cache into `Diagnostics`, using `path` as file name.
    pub(crate) fn into_diagnostics(self, path: &Path) -> Diagnostics {
        let messages = if self.messages.is_empty() {
            Vec::new()
        } else {
            let file = SourceFileBuilder::new(path.to_string_lossy(), self.source).finish();
            self.messages
                .into_iter()
                .map(|msg| Message {
                    kind: msg.kind,
                    range: msg.range,
                    fix: msg.fix,
                    file: file.clone(),
                    noqa_offset: msg.noqa_offset,
                })
                .collect()
        };
        Diagnostics::new(messages, self.imports)
    }
}

/// On disk representation of a diagnostic message.
#[derive(Clone, Deserialize, Debug, Serialize)]
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
