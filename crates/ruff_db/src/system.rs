pub use glob::PatternError;
pub use memory_fs::MemoryFileSystem;

#[cfg(all(feature = "testing", feature = "os"))]
pub use os::testing::UserConfigDirectoryOverrideGuard;

#[cfg(feature = "os")]
pub use os::OsSystem;

use filetime::FileTime;
use ruff_notebook::{Notebook, NotebookError};
use std::error::Error;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::{fmt, io};
pub use test::{DbWithTestSystem, DbWithWritableSystem, InMemorySystem, TestSystem};
use walk_directory::WalkDirectoryBuilder;

use crate::file_revision::FileRevision;

pub use self::path::{
    deduplicate_nested_paths, DeduplicatedNestedPathsIter, SystemPath, SystemPathBuf,
    SystemVirtualPath, SystemVirtualPathBuf,
};

mod memory_fs;
#[cfg(feature = "os")]
mod os;
mod path;
mod test;
pub mod walk_directory;

pub type Result<T> = std::io::Result<T>;

/// The system on which Ruff runs.
///
/// Ruff supports running on the CLI, in a language server, and in a browser (WASM). Each of these
/// host-systems differ in what system operations they support and how they interact with the file system:
/// * Language server:
///    * Reading a file's content should take into account that it might have unsaved changes because it's open in the editor.
///    * Use structured representations for notebooks, making deserializing a notebook from a string unnecessary.
///    * Use their own file watching infrastructure.
/// * WASM (Browser):
///    * There are ways to emulate a file system in WASM but a native memory-filesystem is more efficient.
///    * Doesn't support a current working directory
///    * File watching isn't supported.
///
/// Abstracting the system also enables tests to use a more efficient in-memory file system.
pub trait System: Debug {
    /// Reads the metadata of the file or directory at `path`.
    ///
    /// This function will traverse symbolic links to query information about the destination file.
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata>;

    /// Returns the canonical, absolute form of a path with all intermediate components normalized
    /// and symbolic links resolved.
    ///
    /// # Errors
    /// This function will return an error in the following situations, but is not limited to just these cases:
    /// * `path` does not exist.
    /// * A non-final component in `path` is not a directory.
    /// * the symlink target path is not valid Unicode.
    ///
    /// ## Windows long-paths
    /// Unlike `std::fs::canonicalize`, this function does remove UNC prefixes if possible.
    /// See [dunce::canonicalize] for more information.
    fn canonicalize_path(&self, path: &SystemPath) -> Result<SystemPathBuf>;

    /// Reads the content of the file at `path` into a [`String`].
    fn read_to_string(&self, path: &SystemPath) -> Result<String>;

    /// Reads the content of the file at `path` as a Notebook.
    ///
    /// This method optimizes for the case where the system holds a structured representation of a [`Notebook`],
    /// allowing to skip the notebook deserialization. Systems that don't use a structured
    /// representation fall-back to deserializing the notebook from a string.
    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError>;

    /// Reads the content of the virtual file at `path` into a [`String`].
    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> Result<String>;

    /// Reads the content of the virtual file at `path` as a [`Notebook`].
    fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError>;

    /// Returns `true` if `path` exists.
    fn path_exists(&self, path: &SystemPath) -> bool {
        self.path_metadata(path).is_ok()
    }

    /// Returns `true` if `path` exists on disk using the exact casing as specified in `path` for the parts after `prefix`.
    ///
    /// This is the same as [`Self::path_exists`] on case-sensitive systems.
    ///
    /// ## The use of prefix
    ///
    /// Prefix is only intended as an optimization for systems that can't efficiently check
    /// if an entire path exists with the exact casing as specified in `path`. However,
    /// implementations are allowed to check the casing of the entire path if they can do so efficiently.
    fn path_exists_case_sensitive(&self, path: &SystemPath, prefix: &SystemPath) -> bool;

    /// Returns the [`CaseSensitivity`] of the system's file system.
    fn case_sensitivity(&self) -> CaseSensitivity;

    /// Returns `true` if `path` exists and is a directory.
    fn is_directory(&self, path: &SystemPath) -> bool {
        self.path_metadata(path)
            .is_ok_and(|metadata| metadata.file_type.is_directory())
    }

    /// Returns `true` if `path` exists and is a file.
    fn is_file(&self, path: &SystemPath) -> bool {
        self.path_metadata(path)
            .is_ok_and(|metadata| metadata.file_type.is_file())
    }

    /// Returns the current working directory
    fn current_directory(&self) -> &SystemPath;

    /// Returns the directory path where user configurations are stored.
    ///
    /// Returns `None` if no such convention exists for the system.
    fn user_config_directory(&self) -> Option<SystemPathBuf>;

    /// Iterate over the contents of the directory at `path`.
    ///
    /// The returned iterator must have the following properties:
    /// - It only iterates over the top level of the directory,
    ///   i.e., it does not recurse into subdirectories.
    /// - It skips the current and parent directories (`.` and `..`
    ///   respectively).
    /// - The iterator yields `std::io::Result<DirEntry>` instances.
    ///   For each instance, an `Err` variant may signify that the path
    ///   of the entry was not valid UTF8, in which case it should be an
    ///   [`std::io::Error`] with the ErrorKind set to
    ///   [`std::io::ErrorKind::InvalidData`] and the payload set to a
    ///   [`camino::FromPathBufError`]. It may also indicate that
    ///   "some sort of intermittent IO error occurred during iteration"
    ///   (language taken from the [`std::fs::read_dir`] documentation).
    ///
    /// # Errors
    /// Returns an error:
    /// - if `path` does not exist in the system,
    /// - if `path` does not point to a directory,
    /// - if the process does not have sufficient permissions to
    ///   view the contents of the directory at `path`
    /// - May also return an error in some other situations as well.
    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>> + 'a>>;

    /// Recursively walks the content of `path`.
    ///
    /// It is allowed to pass a `path` that points to a file. In this case, the walker
    /// yields a single entry for that file.
    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder;

    /// Return an iterator that produces all the `Path`s that match the given
    /// pattern using default match options, which may be absolute or relative to
    /// the current working directory.
    ///
    /// This may return an error if the pattern is invalid.
    fn glob(
        &self,
        pattern: &str,
    ) -> std::result::Result<
        Box<dyn Iterator<Item = std::result::Result<SystemPathBuf, GlobError>>>,
        PatternError,
    >;

    fn as_any(&self) -> &dyn std::any::Any;

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub enum CaseSensitivity {
    /// The case sensitivity of the file system is unknown.
    ///
    /// The file system is either case-sensitive or case-insensitive. A caller
    /// should not assume either case.
    #[default]
    Unknown,

    /// The file system is case-sensitive.
    CaseSensitive,

    /// The file system is case-insensitive.
    CaseInsensitive,
}

impl CaseSensitivity {
    /// Returns `true` if the file system is known to be case-sensitive.
    pub const fn is_case_sensitive(self) -> bool {
        matches!(self, Self::CaseSensitive)
    }
}

impl fmt::Display for CaseSensitivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CaseSensitivity::Unknown => f.write_str("unknown"),
            CaseSensitivity::CaseSensitive => f.write_str("case-sensitive"),
            CaseSensitivity::CaseInsensitive => f.write_str("case-insensitive"),
        }
    }
}

/// System trait for non-readonly systems.
pub trait WritableSystem: System {
    /// Writes the given content to the file at the given path.
    fn write_file(&self, path: &SystemPath, content: &str) -> Result<()>;

    /// Creates a directory at `path` as well as any intermediate directories.
    fn create_directory_all(&self, path: &SystemPath) -> Result<()>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    revision: FileRevision,
    permissions: Option<u32>,
    file_type: FileType,
}

impl Metadata {
    pub fn new(revision: FileRevision, permissions: Option<u32>, file_type: FileType) -> Self {
        Self {
            revision,
            permissions,
            file_type,
        }
    }

    pub fn revision(&self) -> FileRevision {
        self.revision
    }

    pub fn permissions(&self) -> Option<u32> {
        self.permissions
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

impl FileType {
    pub const fn is_file(self) -> bool {
        matches!(self, FileType::File)
    }

    pub const fn is_directory(self) -> bool {
        matches!(self, FileType::Directory)
    }

    pub const fn is_symlink(self) -> bool {
        matches!(self, FileType::Symlink)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    path: SystemPathBuf,
    file_type: FileType,
}

impl DirectoryEntry {
    pub fn new(path: SystemPathBuf, file_type: FileType) -> Self {
        Self { path, file_type }
    }

    pub fn into_path(self) -> SystemPathBuf {
        self.path
    }

    pub fn path(&self) -> &SystemPath {
        &self.path
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }
}

/// A glob iteration error.
///
/// This is typically returned when a particular path cannot be read
/// to determine if its contents match the glob pattern. This is possible
/// if the program lacks the appropriate permissions, for example.
#[derive(Debug)]
pub struct GlobError {
    path: PathBuf,
    error: GlobErrorKind,
}

impl GlobError {
    /// The Path that the error corresponds to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> &GlobErrorKind {
        &self.error
    }
}

impl Error for GlobError {}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.error {
            GlobErrorKind::IOError(error) => {
                write!(
                    f,
                    "attempting to read `{}` resulted in an error: {error}",
                    self.path.display(),
                )
            }
            GlobErrorKind::NonUtf8Path => {
                write!(f, "`{}` is not a valid UTF-8 path", self.path.display(),)
            }
        }
    }
}

impl From<glob::GlobError> for GlobError {
    fn from(value: glob::GlobError) -> Self {
        Self {
            path: value.path().to_path_buf(),
            error: GlobErrorKind::IOError(value.into_error()),
        }
    }
}

#[derive(Debug)]
pub enum GlobErrorKind {
    IOError(io::Error),
    NonUtf8Path,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn file_time_now() -> FileTime {
    FileTime::now()
}

#[cfg(target_arch = "wasm32")]
pub fn file_time_now() -> FileTime {
    // Copied from FileTime::from_system_time()
    let time = web_time::SystemTime::now();

    time.duration_since(web_time::UNIX_EPOCH)
        .map(|d| FileTime::from_unix_time(d.as_secs() as i64, d.subsec_nanos()))
        .unwrap_or_else(|e| {
            let until_epoch = e.duration();
            let (sec_offset, nanos) = if until_epoch.subsec_nanos() == 0 {
                (0, 0)
            } else {
                (-1, 1_000_000_000 - until_epoch.subsec_nanos())
            };

            FileTime::from_unix_time(-(until_epoch.as_secs() as i64) + sec_offset, nanos)
        })
}
