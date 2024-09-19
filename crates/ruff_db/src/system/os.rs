use std::sync::Arc;
use std::{any::Any, path::PathBuf};

use filetime::FileTime;

use ruff_notebook::{Notebook, NotebookError};

use crate::system::{
    DirectoryEntry, FileType, Metadata, Result, System, SystemPath, SystemPathBuf,
    SystemVirtualPath,
};

use super::walk_directory::{
    self, DirectoryWalker, WalkDirectoryBuilder, WalkDirectoryConfiguration,
    WalkDirectoryVisitorBuilder, WalkState,
};

/// A system implementation that uses the OS file system.
#[derive(Default, Debug, Clone)]
pub struct OsSystem {
    inner: Arc<OsSystemInner>,
}

#[derive(Default, Debug)]
struct OsSystemInner {
    cwd: SystemPathBuf,
}

impl OsSystem {
    pub fn new(cwd: impl AsRef<SystemPath>) -> Self {
        let cwd = cwd.as_ref();
        assert!(cwd.as_utf8_path().is_absolute());

        Self {
            inner: Arc::new(OsSystemInner {
                cwd: cwd.to_path_buf(),
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
        path.as_utf8_path()
            .canonicalize_utf8()
            .map(SystemPathBuf::from_utf8_path_buf)
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

    fn current_directory(&self) -> &SystemPath {
        &self.inner.cwd
    }

    /// Creates a builder to recursively walk `path`.
    ///
    /// The walker ignores files according to [`ignore::WalkBuilder::standard_filters`]
    /// when setting [`WalkDirectoryBuilder::standard_filters`] to true.
    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        WalkDirectoryBuilder::new(path, OsDirectoryWalker {})
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
