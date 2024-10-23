use crate::system::SystemPathBuf;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use super::{FileType, SystemPath};

/// A builder for constructing a directory recursive traversal.
pub struct WalkDirectoryBuilder {
    /// The implementation that does the directory walking.
    walker: Box<dyn DirectoryWalker>,

    /// The paths that should be walked.
    paths: Vec<SystemPathBuf>,

    ignore_hidden: bool,

    standard_filters: bool,
}

impl WalkDirectoryBuilder {
    pub fn new<W>(path: impl AsRef<SystemPath>, walker: W) -> Self
    where
        W: DirectoryWalker + 'static,
    {
        Self {
            walker: Box::new(walker),
            paths: vec![path.as_ref().to_path_buf()],
            ignore_hidden: true,
            standard_filters: true,
        }
    }

    /// Adds a path that should be traversed recursively.
    ///
    /// Each additional path is traversed recursively.
    /// This should be preferred over building multiple
    /// walkers since it enables reusing resources.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, path: impl AsRef<SystemPath>) -> Self {
        self.paths.push(path.as_ref().to_path_buf());
        self
    }

    /// Whether hidden files should be ignored.
    ///
    /// The definition of what a hidden file depends on the [`System`](super::System) and can be platform-dependent.
    ///
    /// This is enabled by default.
    pub fn ignore_hidden(mut self, hidden: bool) -> Self {
        self.ignore_hidden = hidden;
        self
    }

    /// Enables all the standard ignore filters.
    ///
    /// This toggles, as a group, all the filters that are enabled by default:
    /// * [`hidden`](Self::ignore_hidden)
    /// * Any [`System`](super::System) specific filters according (e.g., respecting `.ignore`, `.gitignore`, files).
    ///
    /// Defaults to `true`.
    pub fn standard_filters(mut self, standard_filters: bool) -> Self {
        self.standard_filters = standard_filters;
        self.ignore_hidden = standard_filters;

        self
    }

    /// Runs the directory traversal and calls the passed `builder` to create visitors
    /// that do the visiting. The walker may run multiple threads to visit the directories.
    pub fn run<'s, F>(self, builder: F)
    where
        F: FnMut() -> FnVisitor<'s>,
    {
        self.visit(&mut FnBuilder { builder });
    }

    /// Runs the directory traversal and calls the passed `builder` to create visitors
    /// that do the visiting. The walker may run multiple threads to visit the directories.
    pub fn visit(self, builder: &mut dyn WalkDirectoryVisitorBuilder) {
        let configuration = WalkDirectoryConfiguration {
            paths: self.paths,
            ignore_hidden: self.ignore_hidden,
            standard_filters: self.standard_filters,
        };

        self.walker.walk(builder, configuration);
    }
}

/// Concrete walker that performs the directory walking.
pub trait DirectoryWalker {
    fn walk(
        &self,
        builder: &mut dyn WalkDirectoryVisitorBuilder,
        configuration: WalkDirectoryConfiguration,
    );
}

/// Creates a visitor for each thread that does the visiting.
pub trait WalkDirectoryVisitorBuilder<'s> {
    fn build(&mut self) -> Box<dyn WalkDirectoryVisitor + 's>;
}

/// Visitor handling the individual directory entries.
pub trait WalkDirectoryVisitor: Send {
    fn visit(&mut self, entry: std::result::Result<DirectoryEntry, Error>) -> WalkState;
}

struct FnBuilder<F> {
    builder: F,
}

impl<'s, F> WalkDirectoryVisitorBuilder<'s> for FnBuilder<F>
where
    F: FnMut() -> FnVisitor<'s>,
{
    fn build(&mut self) -> Box<dyn WalkDirectoryVisitor + 's> {
        let visitor = (self.builder)();
        Box::new(FnVisitorImpl(visitor))
    }
}

type FnVisitor<'s> =
    Box<dyn FnMut(std::result::Result<DirectoryEntry, Error>) -> WalkState + Send + 's>;

struct FnVisitorImpl<'s>(FnVisitor<'s>);

impl WalkDirectoryVisitor for FnVisitorImpl<'_> {
    fn visit(&mut self, entry: std::result::Result<DirectoryEntry, Error>) -> WalkState {
        (self.0)(entry)
    }
}

pub struct WalkDirectoryConfiguration {
    pub paths: Vec<SystemPathBuf>,
    pub ignore_hidden: bool,
    pub standard_filters: bool,
}

/// An entry in a directory.
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub(super) path: SystemPathBuf,
    pub(super) file_type: FileType,
    pub(super) depth: usize,
}

impl DirectoryEntry {
    /// The full path that this entry represents.
    pub fn path(&self) -> &SystemPath {
        &self.path
    }

    /// The full path that this entry represents.
    /// Analogous to [`DirectoryEntry::path`], but moves ownership of the path.
    pub fn into_path(self) -> SystemPathBuf {
        self.path
    }

    /// Return the file type for the file that this entry points to.
    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    /// Returns the depth at which this entry was created relative to the root.
    pub fn depth(&self) -> usize {
        self.depth
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WalkState {
    /// Continue walking as normal
    Continue,

    /// If the entry given is a directory, don't descend into it.
    /// In all other cases, this has no effect.
    Skip,

    /// Quit the entire iterator as soon as possible.
    ///
    /// Note: This is an inherently asynchronous action. It's possible
    /// for more entries to be yielded even after instructing the iterator to quit.
    Quit,
}

pub struct Error {
    pub(super) depth: Option<usize>,
    pub(super) kind: ErrorKind,
}

impl Error {
    pub fn depth(&self) -> Option<usize> {
        self.depth
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ErrorKind::Loop { ancestor, child } => {
                write!(
                    f,
                    "File system loop found: {child} points to an ancestor {ancestor}",
                )
            }
            ErrorKind::Io {
                path: Some(path),
                err,
            } => {
                write!(f, "IO error for operation on {}: {}", path, err)
            }
            ErrorKind::Io { path: None, err } => err.fmt(f),
            ErrorKind::NonUtf8Path { path } => {
                write!(f, "Non-UTF8 path: {}", path.display())
            }
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for Error {}

#[derive(Debug)]
pub enum ErrorKind {
    /// An error that occurs when a file loop is detected when traversing
    /// symbolic links.
    Loop {
        ancestor: SystemPathBuf,
        child: SystemPathBuf,
    },

    /// An error that occurs when doing I/O
    Io {
        path: Option<SystemPathBuf>,
        err: std::io::Error,
    },

    /// A path is not a valid UTF-8 path.
    NonUtf8Path { path: PathBuf },
}

#[cfg(test)]
pub(super) mod tests {
    use crate::system::walk_directory::{DirectoryEntry, Error};
    use crate::system::{FileType, SystemPathBuf};
    use std::collections::BTreeMap;

    /// Test helper that creates a visual representation of the visited directory entries.
    pub(crate) struct DirectoryEntryToString {
        root_path: SystemPathBuf,
        inner: std::sync::Mutex<DirectoryEntryToStringInner>,
    }

    impl DirectoryEntryToString {
        pub(crate) fn new(root_path: SystemPathBuf) -> Self {
            Self {
                root_path,
                inner: std::sync::Mutex::new(DirectoryEntryToStringInner::default()),
            }
        }

        pub(crate) fn write_entry(&self, entry: Result<DirectoryEntry, Error>) {
            let mut inner = self.inner.lock().unwrap();
            let DirectoryEntryToStringInner { errors, visited } = &mut *inner;

            match entry {
                Ok(entry) => {
                    let relative_path = entry
                        .path()
                        .strip_prefix(&self.root_path)
                        .unwrap_or(entry.path());

                    let unix_path = relative_path
                        .components()
                        .map(|component| component.as_str())
                        .collect::<Vec<_>>()
                        .join("/");

                    visited.insert(unix_path, (entry.file_type, entry.depth));
                }
                Err(error) => {
                    errors.push_str(&error.to_string());
                    errors.push('\n');
                }
            }
        }
    }

    impl std::fmt::Display for DirectoryEntryToString {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let inner = self.inner.lock().unwrap();
            write!(f, "{paths:#?}", paths = inner.visited)?;

            if !inner.errors.is_empty() {
                writeln!(f, "\n\n{errors}", errors = inner.errors).unwrap();
            }

            Ok(())
        }
    }

    #[derive(Default)]
    struct DirectoryEntryToStringInner {
        errors: String,
        /// Stores the visited path. The key is the relative path to the root, using `/` as path separator.
        visited: BTreeMap<String, (FileType, usize)>,
    }
}
