#![allow(clippy::disallowed_methods)]

mod ignore;

use self::ignore::IgnoreFiles;
use super::walk_directory::{
    self, DirectoryWalker, IgnoreIncremental, WalkDirectoryBuilder, WalkDirectoryConfiguration,
    WalkDirectoryVisitorBuilder, WalkState,
};
use crate::max_parallelism;
use crate::system::{
    Command, CommandExecutor, DirectoryEntry, FileType, Metadata, Result, System, SystemPath,
    SystemPathBuf, SystemVirtualPath, WhichError, WhichResult, WritableSystem,
};
use filetime::FileTime;
use ruff_notebook::{Notebook, NotebookError};
use std::num::NonZeroUsize;
use std::process::Output;
use std::sync::Arc;
use std::{
    any::Any,
    path::{Path, PathBuf},
};

/// A system implementation that uses the OS file system.
#[derive(Debug, Clone)]
pub struct OsSystem {
    inner: Arc<OsSystemInner>,
}

#[derive(Default, Debug)]
struct OsSystemInner {
    cwd: SystemPathBuf,

    /// Overrides the user's configuration directory for testing.
    /// This is an `Option<Option<..>>` to allow setting an override of `None`.
    #[cfg(feature = "testing")]
    user_config_directory_override: std::sync::Mutex<Option<Option<SystemPathBuf>>>,
}

impl OsSystem {
    pub fn new(cwd: impl AsRef<SystemPath>) -> Self {
        let cwd = cwd.as_ref();
        assert!(cwd.as_utf8_path().is_absolute());

        tracing::debug!(
            "Architecture: {}, OS: {}",
            std::env::consts::ARCH,
            std::env::consts::OS,
        );

        Self {
            // Spreading `..Default` because it isn't possible to feature gate the initializer of a single field.
            inner: Arc::new(OsSystemInner {
                cwd: cwd.to_path_buf(),
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

    fn is_same_file(&self, first: &SystemPath, second: &SystemPath) -> Result<bool> {
        same_file::is_same_file(first.as_std_path(), second.as_std_path())
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

    fn which(&self, name: &str) -> WhichResult {
        let path = which::which(name).map_err(|err| match err {
            which::Error::CannotFindBinaryPath => WhichError::CannotFindBinaryPath,
            which::Error::CannotGetCurrentDirAndPathListEmpty => {
                WhichError::CannotGetCurrentDirAndPathListEmpty
            }
            which::Error::CannotCanonicalize => WhichError::CannotCanonicalize,
        })?;

        match SystemPathBuf::from_path_buf(path) {
            Ok(path) => Ok(path),
            Err(_) => Err(WhichError::NonUtf8Path),
        }
    }

    fn command_executor(&self) -> Option<&dyn CommandExecutor> {
        Some(self)
    }

    fn current_directory(&self) -> &SystemPath {
        &self.inner.cwd
    }

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

    /// Returns an absolute cache directory on the system.
    ///
    /// On Linux and macOS, uses `$XDG_CACHE_HOME/ty` or `.cache/ty`.
    /// On Windows, uses `C:\Users\User\AppData\Local\ty\cache`.
    fn cache_dir(&self) -> Option<SystemPathBuf> {
        use etcetera::BaseStrategy as _;

        let cache_dir = etcetera::base_strategy::choose_base_strategy()
            .ok()
            .map(|dirs| dirs.cache_dir().join("ty"))
            .map(|cache_dir| {
                if cfg!(windows) {
                    // On Windows, we append `cache` to the LocalAppData directory, i.e., prefer
                    // `C:\Users\User\AppData\Local\ty\cache` over `C:\Users\User\AppData\Local\ty`.
                    cache_dir.join("cache")
                } else {
                    cache_dir
                }
            })
            .and_then(|path| SystemPathBuf::from_path_buf(path).ok())
            .unwrap_or_else(|| SystemPathBuf::from(".ty_cache"));

        Some(cache_dir)
    }

    /// Creates a builder to recursively walk `path`.
    ///
    /// The walker ignores files according to [`::ignore::WalkBuilder::standard_filters`]
    /// when setting [`WalkDirectoryBuilder::standard_filters`] to true.
    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        WalkDirectoryBuilder::new(
            path,
            OsDirectoryWalker {
                cwd: self.current_directory().to_path_buf(),
            },
        )
    }

    fn as_writable(&self) -> Option<&dyn WritableSystem> {
        Some(self)
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

    fn env_var(&self, name: &str) -> std::result::Result<String, std::env::VarError> {
        std::env::var(name)
    }

    fn dyn_clone(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }
}

impl CommandExecutor for OsSystem {
    fn execute(&self, command: Command) -> Result<Output> {
        let directory = command
            .get_current_dir()
            .unwrap_or_else(|| self.current_directory());

        // `posix_spawn` is supposed to surface execve/chdir failures as `spawn`/`output`
        // errors. Some implementations instead report a successful spawn and a child
        // exit status of 127, including qemu-user, OpenBSD, and HPPA. Check the
        // preconditions we already know so those cases stay `io::Error`s.
        check_spawn_preconditions(command.get_executable(), directory.as_std_path())?;

        std::process::Command::new(command.get_executable())
            .args(command.get_args())
            .current_dir(directory.as_std_path())
            .output()
    }

    fn dyn_clone(&self) -> Box<dyn CommandExecutor> {
        Box::new(self.clone())
    }
}

impl WritableSystem for OsSystem {
    fn create_new_file(&self, path: &SystemPath) -> Result<()> {
        std::fs::File::create_new(path).map(drop)
    }

    fn write_file_bytes(&self, path: &SystemPath, content: &[u8]) -> Result<()> {
        std::fs::write(path.as_std_path(), content)
    }

    fn create_directory_all(&self, path: &SystemPath) -> Result<()> {
        std::fs::create_dir_all(path.as_std_path())
    }

    fn dyn_clone(&self) -> Box<dyn WritableSystem> {
        Box::new(self.clone())
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

#[derive(Debug)]
struct OsDirectoryWalker {
    cwd: SystemPathBuf,
}

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

        let mut builder = ::ignore::WalkBuilder::new(first.as_std_path());
        builder.current_dir(self.cwd.as_std_path());

        builder.standard_filters(standard_filters);
        builder.hidden(hidden);

        for additional_path in additional {
            builder.add(additional_path.as_std_path());
        }

        builder.threads(max_parallelism().min(NonZeroUsize::new(12).unwrap()).get());

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
                                ::ignore::WalkState::Skip
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
                            ::ignore::WalkState::Continue
                        }
                    },
                }
            })
        });
    }

    fn incremental_matcher(
        &self,
        configuration: WalkDirectoryConfiguration,
    ) -> Box<dyn IgnoreIncremental> {
        let WalkDirectoryConfiguration {
            paths,
            ignore_hidden: hidden,
            standard_filters,
        } = configuration;

        let mut builder = ::ignore::WalkBuilder::from_iter(paths.iter().map(|p| p.as_std_path()));
        builder.current_dir(self.cwd.as_std_path());
        builder.standard_filters(standard_filters);
        builder.hidden(hidden);
        let root_matchers = builder.build_matchers();
        Box::new(IgnoreFiles { root_matchers })
    }
}

#[cold]
fn ignore_to_walk_directory_error(
    error: ::ignore::Error,
    path: Option<PathBuf>,
    depth: Option<usize>,
) -> std::result::Result<walk_directory::Error, ::ignore::Error> {
    use ::ignore::Error;

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

impl From<WalkState> for ::ignore::WalkState {
    fn from(value: WalkState) -> Self {
        match value {
            WalkState::Continue => ::ignore::WalkState::Continue,
            WalkState::Skip => ::ignore::WalkState::Skip,
            WalkState::Quit => ::ignore::WalkState::Quit,
        }
    }
}

fn not_found() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
}

/// Rejects `current_dir` and program values that would fail `execve`/`chdir`.
///
/// Keep the error kinds and messages aligned with a native `Command::output()`
/// failure so CLI snapshots stay stable across platforms.
fn check_spawn_preconditions(executable: &str, current_dir: &Path) -> Result<()> {
    let current_dir_metadata = std::fs::metadata(current_dir)?;
    if !current_dir_metadata.is_dir() {
        return Err(not_a_directory());
    }

    if program_is_path(executable) {
        ensure_executable_path(Path::new(executable), current_dir)
    } else if which::which_in(executable, std::env::var_os("PATH"), current_dir).is_ok() {
        Ok(())
    } else {
        Err(program_not_found())
    }
}

/// Returns `true` when `program` is a path rather than a `PATH` lookup name.
///
/// This matches `execvp(3)`: a slash (or, on Windows, a backslash) skips `PATH`.
fn program_is_path(program: &str) -> bool {
    let path = Path::new(program);
    path.is_absolute() || path.components().nth(1).is_some()
}

fn ensure_executable_path(program: &Path, current_dir: &Path) -> Result<()> {
    let path = if program.is_absolute() {
        program.to_path_buf()
    } else {
        current_dir.join(program)
    };
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() || !is_executable(&metadata) {
        return Err(permission_denied());
    }
    Ok(())
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    true
}

fn program_not_found() -> std::io::Error {
    #[cfg(windows)]
    {
        std::io::Error::new(std::io::ErrorKind::NotFound, "program not found")
    }
    #[cfg(not(windows))]
    {
        // Match `Command::output()` on Unix: "No such file or directory (os error 2)".
        std::io::Error::from_raw_os_error(2)
    }
}

fn permission_denied() -> std::io::Error {
    #[cfg(unix)]
    {
        std::io::Error::from_raw_os_error(13)
    }
    #[cfg(not(unix))]
    {
        std::io::Error::from(std::io::ErrorKind::PermissionDenied)
    }
}

fn not_a_directory() -> std::io::Error {
    #[cfg(unix)]
    {
        std::io::Error::from_raw_os_error(20)
    }
    #[cfg(not(unix))]
    {
        std::io::Error::new(std::io::ErrorKind::NotADirectory, "Not a directory")
    }
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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::system::DirectoryEntry;
    use crate::system::walk_directory::tests::DirectoryEntryToString;

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

    #[test]
    fn execute_missing_bare_program_is_not_found() {
        let tempdir = TempDir::new().unwrap();
        let system = OsSystem::new(SystemPath::from_std_path(tempdir.path()).unwrap());
        let error = system
            .run_command(Command::new("missing-ty-uv-executable-7f3a9b2c"))
            .expect_err("missing programs should fail as spawn errors");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn execute_missing_absolute_program_is_not_found() {
        let tempdir = TempDir::new().unwrap();
        let system = OsSystem::new(SystemPath::from_std_path(tempdir.path()).unwrap());
        let missing = tempdir.path().join("missing-ty-uv-executable-7f3a9b2c");
        let error = system
            .run_command(Command::new(missing.to_str().unwrap()))
            .expect_err("missing programs should fail as spawn errors");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn execute_missing_current_dir_is_not_found() {
        let tempdir = TempDir::new().unwrap();
        let system = OsSystem::new(SystemPath::from_std_path(tempdir.path()).unwrap());
        let missing_path = tempdir.path().join("no-such-dir-7f3a9b2c");
        let missing_dir = SystemPath::from_std_path(&missing_path).unwrap();
        let mut command = Command::new("missing-ty-uv-executable-7f3a9b2c");
        command.current_dir(missing_dir);
        let error = system
            .run_command(command)
            .expect_err("missing current_dir should fail as spawn errors");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn execute_file_current_dir_is_not_a_directory() {
        let tempdir = TempDir::new().unwrap();
        std::fs::write(tempdir.path().join("not-a-dir"), b"").unwrap();
        let system = OsSystem::new(SystemPath::from_std_path(tempdir.path()).unwrap());
        let file_path = tempdir.path().join("not-a-dir");
        let file_dir = SystemPath::from_std_path(&file_path).unwrap();
        let mut command = Command::new("missing-ty-uv-executable-7f3a9b2c");
        command.current_dir(file_dir);
        let error = system
            .run_command(command)
            .expect_err("non-directory current_dir should fail as spawn errors");
        assert!(error.to_string().contains("Not a directory"));
    }

    #[cfg(unix)]
    #[test]
    fn execute_directory_program_is_permission_denied() {
        let tempdir = TempDir::new().unwrap();
        let system = OsSystem::new(SystemPath::from_std_path(tempdir.path()).unwrap());
        let error = system
            .run_command(Command::new(tempdir.path().to_str().unwrap()))
            .expect_err("directory programs should fail as spawn errors");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[cfg(unix)]
    #[test]
    fn execute_non_executable_program_is_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let tempdir = TempDir::new().unwrap();
        let program = tempdir.path().join("not-executable");
        std::fs::write(&program, b"").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).unwrap();
        let system = OsSystem::new(SystemPath::from_std_path(tempdir.path()).unwrap());
        let error = system
            .run_command(Command::new(program.to_str().unwrap()))
            .expect_err("non-executable programs should fail as spawn errors");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }
}
