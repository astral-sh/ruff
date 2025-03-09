use filetime::FileTime;
use ruff_notebook::{Notebook, NotebookError};
use rustc_hash::FxHashSet;
use std::panic::RefUnwindSafe;
use std::sync::Arc;
use std::{any::Any, path::PathBuf};

use crate::system::{
    CaseSensitivity, DirectoryEntry, FileType, GlobError, GlobErrorKind, Metadata, Result, System,
    SystemPath, SystemPathBuf, SystemVirtualPath, WritableSystem,
};

use super::walk_directory::{
    self, DirectoryWalker, WalkDirectoryBuilder, WalkDirectoryConfiguration,
    WalkDirectoryVisitorBuilder, WalkState,
};

/// A system implementation that uses the OS file system.
#[derive(Debug, Clone)]
pub struct OsSystem {
    inner: Arc<OsSystemInner>,
}

#[derive(Default, Debug)]
struct OsSystemInner {
    cwd: SystemPathBuf,

    real_case_cache: CaseSensitivePathsCache,

    case_sensitivity: CaseSensitivity,

    /// Overrides the user's configuration directory for testing.
    /// This is an `Option<Option<..>>` to allow setting an override of `None`.
    #[cfg(feature = "testing")]
    user_config_directory_override: std::sync::Mutex<Option<Option<SystemPathBuf>>>,
}

impl OsSystem {
    pub fn new(cwd: impl AsRef<SystemPath>) -> Self {
        let cwd = cwd.as_ref();
        assert!(cwd.as_utf8_path().is_absolute());

        let case_sensitivity = detect_case_sensitivity(cwd);

        tracing::debug!(
            "Architecture: {}, OS: {}, case-sensitive: {case_sensitivity}",
            std::env::consts::ARCH,
            std::env::consts::OS,
        );

        Self {
            // Spreading `..Default` because it isn't possible to feature gate the initializer of a single field.
            #[allow(clippy::needless_update)]
            inner: Arc::new(OsSystemInner {
                cwd: cwd.to_path_buf(),
                case_sensitivity,
                ..Default::default()
            }),
        }
    }

    #[cfg(unix)]
    fn permissions(metadata: &std::fs::Metadata) -> Option<u32> {
        use std::os::unix::fs::PermissionsExt;

        Some(metadata.permissions().mode())
    }

    #[cfg(not(unix))]
    fn permissions(_metadata: &std::fs::Metadata) -> Option<u32> {
        None
    }
}

impl System for OsSystem {
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata> {
        let metadata = path.as_std_path().metadata()?;
        let last_modified = FileTime::from_last_modification_time(&metadata);

        Ok(Metadata {
            revision: last_modified.into(),
            permissions: Self::permissions(&metadata),
            file_type: metadata.file_type().into(),
        })
    }

    fn canonicalize_path(&self, path: &SystemPath) -> Result<SystemPathBuf> {
        path.as_utf8_path().canonicalize_utf8().map(|path| {
            SystemPathBuf::from_utf8_path_buf(path)
                .simplified()
                .to_path_buf()
        })
    }

    fn read_to_string(&self, path: &SystemPath) -> Result<String> {
        std::fs::read_to_string(path.as_std_path())
    }

    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError> {
        Notebook::from_path(path.as_std_path())
    }

    fn read_virtual_path_to_string(&self, _path: &SystemVirtualPath) -> Result<String> {
        Err(not_found())
    }

    fn read_virtual_path_to_notebook(
        &self,
        _path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError> {
        Err(NotebookError::from(not_found()))
    }

    fn path_exists(&self, path: &SystemPath) -> bool {
        path.as_std_path().exists()
    }

    fn path_exists_case_sensitive(&self, path: &SystemPath, prefix: &SystemPath) -> bool {
        if self.case_sensitivity().is_case_sensitive() {
            self.path_exists(path)
        } else {
            self.path_exists_case_sensitive_fast(path)
                .unwrap_or_else(|| self.path_exists_case_sensitive_slow(path, prefix))
        }
    }

    fn case_sensitivity(&self) -> CaseSensitivity {
        self.inner.case_sensitivity
    }

    fn current_directory(&self) -> &SystemPath {
        &self.inner.cwd
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        // In testing, we allow overriding the user configuration directory by using a
        // thread local because overriding the environment variables breaks test isolation
        // (tests run concurrently) and mutating environment variable in a multithreaded
        // application is inherently unsafe.
        #[cfg(feature = "testing")]
        if let Ok(directory_override) = self.try_get_user_config_directory_override() {
            return directory_override;
        }

        use etcetera::BaseStrategy as _;

        let strategy = etcetera::base_strategy::choose_base_strategy().ok()?;
        SystemPathBuf::from_path_buf(strategy.config_dir()).ok()
    }

    // TODO: Remove this feature gating once `ruff_wasm` no longer indirectly depends on `ruff_db` with the
    //   `os` feature enabled (via `ruff_workspace` -> `ruff_graph` -> `ruff_db`).
    #[cfg(target_arch = "wasm32")]
    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        #[cfg(feature = "testing")]
        if let Ok(directory_override) = self.try_get_user_config_directory_override() {
            return directory_override;
        }

        None
    }

    /// Creates a builder to recursively walk `path`.
    ///
    /// The walker ignores files according to [`ignore::WalkBuilder::standard_filters`]
    /// when setting [`WalkDirectoryBuilder::standard_filters`] to true.
    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        WalkDirectoryBuilder::new(path, OsDirectoryWalker {})
    }

    fn glob(
        &self,
        pattern: &str,
    ) -> std::result::Result<
        Box<dyn Iterator<Item = std::result::Result<SystemPathBuf, GlobError>>>,
        glob::PatternError,
    > {
        glob::glob(pattern).map(|inner| {
            let iterator = inner.map(|result| {
                let path = result?;

                let system_path = SystemPathBuf::from_path_buf(path).map_err(|path| GlobError {
                    path,
                    error: GlobErrorKind::NonUtf8Path,
                })?;

                Ok(system_path)
            });

            let boxed: Box<dyn Iterator<Item = _>> = Box::new(iterator);
            boxed
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn read_directory(
        &self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>>>> {
        Ok(Box::new(path.as_utf8_path().read_dir_utf8()?.map(|res| {
            let res = res?;

            let file_type = res.file_type()?;
            Ok(DirectoryEntry {
                path: SystemPathBuf::from_utf8_path_buf(res.into_path()),
                file_type: file_type.into(),
            })
        })))
    }
}

impl OsSystem {
    /// Path sensitive testing if a path exists by canonicalization the path and comparing it with `path`.
    ///
    /// This is faster than the slow path, because it requires a single system call for each path
    /// instead of at least one system call for each component between `path` and `prefix`.
    ///
    /// However, using `canonicalize` to resolve the path's casing doesn't work in two cases:
    /// * if `path` is a symlink because `canonicalize` then returns the symlink's target and not the symlink's source path.
    /// * on Windows: If `path` is a mapped network drive because `canonicalize` then returns the UNC path
    ///   (e.g. `Z:\` is mapped to `\\server\share` and `canonicalize` then returns `\\?\UNC\server\share`).
    ///
    /// Symlinks and mapped network drives should be rare enough that this fast path is worth trying first,
    /// even if it comes at a cost for those rare use cases.
    fn path_exists_case_sensitive_fast(&self, path: &SystemPath) -> Option<bool> {
        // This is a more forgiving version of `dunce::simplified` that removes all `\\?\` prefixes on Windows.
        // We use this more forgiving version because we don't intend on using either path for anything other than comparison
        // and the prefix is only relevant when passing the path to other programs and its longer than 200 something
        // characters.
        fn simplify_ignore_verbatim(path: &SystemPath) -> &SystemPath {
            if cfg!(windows) {
                if path.as_utf8_path().as_str().starts_with(r"\\?\") {
                    SystemPath::new(&path.as_utf8_path().as_str()[r"\\?\".len()..])
                } else {
                    path
                }
            } else {
                path
            }
        }

        let simplified = simplify_ignore_verbatim(path);

        let Ok(canonicalized) = simplified.as_std_path().canonicalize() else {
            // The path doesn't exist or can't be accessed. The path doesn't exist.
            return Some(false);
        };

        let Ok(canonicalized) = SystemPathBuf::from_path_buf(canonicalized) else {
            // The original path is valid UTF8 but the canonicalized path isn't. This definitely suggests
            // that a symlink is involved. Fall back to the slow path.
            tracing::debug!("Falling back to the slow case-sensitive path existence check because the canonicalized path of `{simplified}` is not valid UTF-8");
            return None;
        };

        let simplified_canonicalized = simplify_ignore_verbatim(&canonicalized);

        // Test if the paths differ by anything other than casing. If so, that suggests that
        // `path` pointed to a symlink (or some other none reversible path normalization happened).
        // In this case, fall back to the slow path.
        if simplified_canonicalized.as_str().to_lowercase() != simplified.as_str().to_lowercase() {
            tracing::debug!("Falling back to the slow case-sensitive path existence check for `{simplified}` because the canonicalized path `{simplified_canonicalized}` differs not only by casing");
            return None;
        }

        // If there are no symlinks involved, then `path` exists only if it is the same as the canonicalized path.
        Some(simplified_canonicalized == simplified)
    }

    fn path_exists_case_sensitive_slow(&self, path: &SystemPath, prefix: &SystemPath) -> bool {
        // Iterate over the sub-paths up to prefix and check if they match the casing as on disk.
        for ancestor in path.ancestors() {
            if ancestor == prefix {
                break;
            }

            match self.inner.real_case_cache.has_name_case(ancestor) {
                Ok(true) => {
                    // Component has correct casing, continue with next component
                }
                Ok(false) => {
                    // Component has incorrect casing
                    return false;
                }
                Err(_) => {
                    // Directory doesn't exist or can't be accessed. We can assume that the file with
                    // the given casing doesn't exist.
                    return false;
                }
            }
        }

        true
    }
}

impl WritableSystem for OsSystem {
    fn write_file(&self, path: &SystemPath, content: &str) -> Result<()> {
        std::fs::write(path.as_std_path(), content)
    }

    fn create_directory_all(&self, path: &SystemPath) -> Result<()> {
        std::fs::create_dir_all(path.as_std_path())
    }
}

impl Default for OsSystem {
    fn default() -> Self {
        Self::new(
            SystemPathBuf::from_path_buf(std::env::current_dir().unwrap_or_default())
                .unwrap_or_default(),
        )
    }
}

#[derive(Debug, Default)]
struct CaseSensitivePathsCache {
    by_lower_case: dashmap::DashMap<SystemPathBuf, ListedDirectory>,
}

impl CaseSensitivePathsCache {
    /// Test if `path`'s file name uses the exact same casing as the file on disk.
    ///
    /// Returns `false` if the file doesn't exist.
    ///
    /// Components other than the file portion are ignored.
    fn has_name_case(&self, path: &SystemPath) -> Result<bool> {
        let Some(parent) = path.parent() else {
            // The root path is always considered to exist.
            return Ok(true);
        };

        let Some(file_name) = path.file_name() else {
            // We can only get here for paths ending in `..` or the root path. Root paths are handled above.
            // Return `true` for paths ending in `..` because `..` is the same regardless of casing.
            return Ok(true);
        };

        let lower_case_path = SystemPathBuf::from(parent.as_str().to_lowercase());
        let last_modification_time =
            FileTime::from_last_modification_time(&parent.as_std_path().metadata()?);

        let entry = self.by_lower_case.entry(lower_case_path);

        if let dashmap::Entry::Occupied(entry) = &entry {
            // Only do a cached lookup if the directory hasn't changed.
            if entry.get().last_modification_time == last_modification_time {
                tracing::trace!("Use cached case-sensitive entry for directory `{}`", parent);
                return Ok(entry.get().names.contains(file_name));
            }
        }

        tracing::trace!(
            "Reading directory `{}` for its case-sensitive filenames",
            parent
        );
        let start = std::time::Instant::now();
        let mut names = FxHashSet::default();

        for entry in parent.as_std_path().read_dir()? {
            let Ok(entry) = entry else {
                continue;
            };

            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };

            names.insert(name.into_boxed_str());
        }

        let directory = entry.insert(ListedDirectory {
            last_modification_time,
            names,
        });

        tracing::debug!(
            "Caching the case-sensitive paths for directory `{parent}` took {:?}",
            start.elapsed()
        );

        Ok(directory.names.contains(file_name))
    }
}

impl RefUnwindSafe for CaseSensitivePathsCache {}

#[derive(Debug, Eq, PartialEq)]
struct ListedDirectory {
    last_modification_time: FileTime,
    names: FxHashSet<Box<str>>,
}

#[derive(Debug)]
struct OsDirectoryWalker;

impl DirectoryWalker for OsDirectoryWalker {
    fn walk(
        &self,
        visitor_builder: &mut dyn WalkDirectoryVisitorBuilder,
        configuration: WalkDirectoryConfiguration,
    ) {
        let WalkDirectoryConfiguration {
            paths,
            ignore_hidden: hidden,
            standard_filters,
        } = configuration;

        let Some((first, additional)) = paths.split_first() else {
            return;
        };

        let mut builder = ignore::WalkBuilder::new(first.as_std_path());

        builder.standard_filters(standard_filters);
        builder.hidden(hidden);

        for additional_path in additional {
            builder.add(additional_path.as_std_path());
        }

        builder.threads(
            std::thread::available_parallelism()
                .map_or(1, std::num::NonZeroUsize::get)
                .min(12),
        );

        builder.build_parallel().run(|| {
            let mut visitor = visitor_builder.build();

            Box::new(move |entry| {
                match entry {
                    Ok(entry) => {
                        // SAFETY: The walkdir crate supports `stdin` files and `file_type` can be `None` for these files.
                        //   We don't make use of this feature, which is why unwrapping here is ok.
                        let file_type = entry.file_type().unwrap();
                        let depth = entry.depth();

                        // `walkdir` reports errors related to parsing ignore files as part of the entry.
                        // These aren't fatal for us. We should keep going even if an ignore file contains a syntax error.
                        // But we log the error here for better visibility (same as ripgrep, Ruff ignores it)
                        if let Some(error) = entry.error() {
                            tracing::warn!("{error}");
                        }

                        match SystemPathBuf::from_path_buf(entry.into_path()) {
                            Ok(path) => {
                                let directory_entry = walk_directory::DirectoryEntry {
                                    path,
                                    file_type: file_type.into(),
                                    depth,
                                };

                                visitor.visit(Ok(directory_entry)).into()
                            }
                            Err(path) => {
                                visitor.visit(Err(walk_directory::Error {
                                    depth: Some(depth),
                                    kind: walk_directory::ErrorKind::NonUtf8Path { path },
                                }));

                                // Skip the entire directory because all the paths won't be UTF-8 paths.
                                ignore::WalkState::Skip
                            }
                        }
                    }
                    Err(error) => match ignore_to_walk_directory_error(error, None, None) {
                        Ok(error) => visitor.visit(Err(error)).into(),
                        Err(error) => {
                            // This should only be reached when the error is a `.ignore` file related error
                            // (which, should not be reported here but the `ignore` crate doesn't distinguish between ignore and IO errors).
                            // Let's log the error to at least make it visible.
                            tracing::warn!("Failed to traverse directory: {error}.");
                            ignore::WalkState::Continue
                        }
                    },
                }
            })
        });
    }
}

#[cold]
fn ignore_to_walk_directory_error(
    error: ignore::Error,
    path: Option<PathBuf>,
    depth: Option<usize>,
) -> std::result::Result<walk_directory::Error, ignore::Error> {
    use ignore::Error;

    match error {
        Error::WithPath { path, err } => ignore_to_walk_directory_error(*err, Some(path), depth),
        Error::WithDepth { err, depth } => ignore_to_walk_directory_error(*err, path, Some(depth)),
        Error::WithLineNumber { err, .. } => ignore_to_walk_directory_error(*err, path, depth),
        Error::Loop { child, ancestor } => {
            match (
                SystemPathBuf::from_path_buf(child),
                SystemPathBuf::from_path_buf(ancestor),
            ) {
                (Ok(child), Ok(ancestor)) => Ok(walk_directory::Error {
                    depth,
                    kind: walk_directory::ErrorKind::Loop { child, ancestor },
                }),
                (Err(child), _) => Ok(walk_directory::Error {
                    depth,
                    kind: walk_directory::ErrorKind::NonUtf8Path { path: child },
                }),
                // We should never reach this because we should never traverse into a non UTF8 path but handle it anyway.
                (_, Err(ancestor)) => Ok(walk_directory::Error {
                    depth,
                    kind: walk_directory::ErrorKind::NonUtf8Path { path: ancestor },
                }),
            }
        }

        Error::Io(err) => match path.map(SystemPathBuf::from_path_buf).transpose() {
            Ok(path) => Ok(walk_directory::Error {
                depth,
                kind: walk_directory::ErrorKind::Io { path, err },
            }),
            Err(path) => Ok(walk_directory::Error {
                depth,
                kind: walk_directory::ErrorKind::NonUtf8Path { path },
            }),
        },

        // Ignore related errors, we warn about them but we don't abort iteration because of them.
        error @ (Error::Glob { .. }
        | Error::UnrecognizedFileType(_)
        | Error::InvalidDefinition
        | Error::Partial(..)) => Err(error),
    }
}

impl From<std::fs::FileType> for FileType {
    fn from(file_type: std::fs::FileType) -> Self {
        if file_type.is_file() {
            FileType::File
        } else if file_type.is_dir() {
            FileType::Directory
        } else {
            FileType::Symlink
        }
    }
}

impl From<WalkState> for ignore::WalkState {
    fn from(value: WalkState) -> Self {
        match value {
            WalkState::Continue => ignore::WalkState::Continue,
            WalkState::Skip => ignore::WalkState::Skip,
            WalkState::Quit => ignore::WalkState::Quit,
        }
    }
}

fn not_found() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
}

#[cfg(feature = "testing")]
pub(super) mod testing {

    use crate::system::{OsSystem, SystemPathBuf};

    impl OsSystem {
        /// Overrides the user configuration directory for the current scope
        /// (for as long as the returned override is not dropped).
        pub fn with_user_config_directory(
            &self,
            directory: Option<SystemPathBuf>,
        ) -> UserConfigDirectoryOverrideGuard {
            let mut directory_override = self.inner.user_config_directory_override.lock().unwrap();
            let previous = directory_override.replace(directory);

            UserConfigDirectoryOverrideGuard {
                previous,
                system: self.clone(),
            }
        }

        /// Returns [`Ok`] if any override is set and [`Err`] otherwise.
        pub(super) fn try_get_user_config_directory_override(
            &self,
        ) -> Result<Option<SystemPathBuf>, ()> {
            let directory_override = self.inner.user_config_directory_override.lock().unwrap();
            match directory_override.as_ref() {
                Some(directory_override) => Ok(directory_override.clone()),
                None => Err(()),
            }
        }
    }

    /// A scoped override of the [user's configuration directory](crate::System::user_config_directory) for the [`OsSystem`].
    ///
    /// Prefer overriding the user's configuration directory for tests that require
    /// spawning a new process (e.g. CLI tests) by setting the `APPDATA` (windows)
    /// or `XDG_CONFIG_HOME` (unix and other platforms) environment variables.
    /// For example, by setting the environment variables when invoking the CLI with insta.
    ///
    /// Requires the `testing` feature.
    #[must_use]
    pub struct UserConfigDirectoryOverrideGuard {
        previous: Option<Option<SystemPathBuf>>,
        system: OsSystem,
    }

    impl Drop for UserConfigDirectoryOverrideGuard {
        fn drop(&mut self) {
            if let Ok(mut directory_override) =
                self.system.inner.user_config_directory_override.try_lock()
            {
                *directory_override = self.previous.take();
            }
        }
    }
}

#[cfg(not(unix))]
fn detect_case_sensitivity(_path: &SystemPath) -> CaseSensitivity {
    // 99% of windows systems aren't case sensitive Don't bother checking.
    CaseSensitivity::Unknown
}

#[cfg(unix)]
fn detect_case_sensitivity(path: &SystemPath) -> CaseSensitivity {
    use std::os::unix::fs::MetadataExt;

    let Ok(original_case_metadata) = path.as_std_path().metadata() else {
        return CaseSensitivity::Unknown;
    };

    let upper_case = SystemPathBuf::from(path.as_str().to_uppercase());
    if &*upper_case == path {
        return CaseSensitivity::Unknown;
    }

    match upper_case.as_std_path().metadata() {
        Ok(uppercase_meta) => {
            // The file system is case insensitive if the upper case and mixed case paths have the same inode.
            if uppercase_meta.ino() == original_case_metadata.ino() {
                CaseSensitivity::CaseInsensitive
            } else {
                CaseSensitivity::CaseSensitive
            }
        }
        // In the error case, the file system is case sensitive if the file in all upper case doesn't exist.
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                CaseSensitivity::CaseSensitive
            } else {
                CaseSensitivity::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::system::walk_directory::tests::DirectoryEntryToString;
    use crate::system::DirectoryEntry;

    use super::*;

    #[test]
    fn read_directory() {
        let tempdir = TempDir::new().unwrap();
        let tempdir_path = tempdir.path();
        std::fs::create_dir_all(tempdir_path.join("a/foo")).unwrap();
        let files = &["b.ts", "a/bar.py", "d.rs", "a/foo/bar.py", "a/baz.pyi"];
        for path in files {
            std::fs::File::create(tempdir_path.join(path)).unwrap();
        }

        let tempdir_path = SystemPath::from_std_path(tempdir_path).unwrap();
        let fs = OsSystem::new(tempdir_path);

        let mut sorted_contents: Vec<DirectoryEntry> = fs
            .read_directory(&tempdir_path.join("a"))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        sorted_contents.sort_by(|a, b| a.path.cmp(&b.path));

        let expected_contents = vec![
            DirectoryEntry::new(tempdir_path.join("a/bar.py"), FileType::File),
            DirectoryEntry::new(tempdir_path.join("a/baz.pyi"), FileType::File),
            DirectoryEntry::new(tempdir_path.join("a/foo"), FileType::Directory),
        ];
        assert_eq!(sorted_contents, expected_contents)
    }

    #[test]
    fn read_directory_nonexistent() {
        let tempdir = TempDir::new().unwrap();

        let fs = OsSystem::new(SystemPath::from_std_path(tempdir.path()).unwrap());
        let result = fs.read_directory(SystemPath::new("doesnt_exist"));
        assert!(result.is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound));
    }

    #[test]
    fn read_directory_on_file() {
        let tempdir = TempDir::new().unwrap();
        let tempdir_path = tempdir.path();
        std::fs::File::create(tempdir_path.join("a.py")).unwrap();

        let tempdir_path = SystemPath::from_std_path(tempdir_path).unwrap();
        let fs = OsSystem::new(tempdir_path);
        let result = fs.read_directory(&tempdir_path.join("a.py"));
        let Err(error) = result else {
            panic!("Expected the read_dir() call to fail!");
        };

        // We can't assert the error kind here because it's apparently an unstable feature!
        // https://github.com/rust-lang/rust/issues/86442
        // assert_eq!(error.kind(), std::io::ErrorKind::NotADirectory);

        // We can't even assert the error message on all platforms, as it's different on Windows,
        // where the message is "The directory name is invalid" rather than "Not a directory".
        if cfg!(unix) {
            assert!(error.to_string().contains("Not a directory"));
        }
    }

    #[test]
    fn walk_directory() -> std::io::Result<()> {
        let tempdir = TempDir::new()?;

        let root = tempdir.path();
        std::fs::create_dir_all(root.join("a/b"))?;
        std::fs::write(root.join("foo.py"), "print('foo')")?;
        std::fs::write(root.join("a/bar.py"), "print('bar')")?;
        std::fs::write(root.join("a/baz.py"), "print('baz')")?;
        std::fs::write(root.join("a/b/c.py"), "print('c')")?;

        let root_sys = SystemPath::from_std_path(root).unwrap();
        let system = OsSystem::new(root_sys);

        let writer = DirectoryEntryToString::new(root_sys.to_path_buf());

        system.walk_directory(root_sys).run(|| {
            Box::new(|entry| {
                writer.write_entry(entry);

                WalkState::Continue
            })
        });

        assert_eq!(
            writer.to_string(),
            r#"{
    "": (
        Directory,
        0,
    ),
    "a": (
        Directory,
        1,
    ),
    "a/b": (
        Directory,
        2,
    ),
    "a/b/c.py": (
        File,
        3,
    ),
    "a/bar.py": (
        File,
        2,
    ),
    "a/baz.py": (
        File,
        2,
    ),
    "foo.py": (
        File,
        1,
    ),
}"#
        );

        Ok(())
    }

    #[test]
    fn walk_directory_ignore() -> std::io::Result<()> {
        let tempdir = TempDir::new()?;

        let root = tempdir.path();
        std::fs::create_dir_all(root.join("a/b"))?;
        std::fs::write(root.join("foo.py"), "print('foo')\n")?;
        std::fs::write(root.join("a/bar.py"), "print('bar')\n")?;
        std::fs::write(root.join("a/baz.py"), "print('baz')\n")?;

        // Exclude the `b` directory.
        std::fs::write(root.join("a/.ignore"), "b/\n")?;
        std::fs::write(root.join("a/b/c.py"), "print('c')\n")?;

        let root_sys = SystemPath::from_std_path(root).unwrap();
        let system = OsSystem::new(root_sys);

        let writer = DirectoryEntryToString::new(root_sys.to_path_buf());

        system
            .walk_directory(root_sys)
            .standard_filters(true)
            .run(|| {
                Box::new(|entry| {
                    writer.write_entry(entry);
                    WalkState::Continue
                })
            });

        assert_eq!(
            writer.to_string(),
            r#"{
    "": (
        Directory,
        0,
    ),
    "a": (
        Directory,
        1,
    ),
    "a/bar.py": (
        File,
        2,
    ),
    "a/baz.py": (
        File,
        2,
    ),
    "foo.py": (
        File,
        1,
    ),
}"#
        );

        Ok(())
    }

    #[test]
    fn walk_directory_file() -> std::io::Result<()> {
        let tempdir = TempDir::new()?;

        let root = tempdir.path();
        std::fs::write(root.join("foo.py"), "print('foo')\n")?;

        let root_sys = SystemPath::from_std_path(root).unwrap();
        let system = OsSystem::new(root_sys);

        let writer = DirectoryEntryToString::new(root_sys.to_path_buf());

        system
            .walk_directory(&root_sys.join("foo.py"))
            .standard_filters(true)
            .run(|| {
                Box::new(|entry| {
                    writer.write_entry(entry);
                    WalkState::Continue
                })
            });

        assert_eq!(
            writer.to_string(),
            r#"{
    "foo.py": (
        File,
        0,
    ),
}"#
        );

        Ok(())
    }
}
