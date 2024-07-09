pub use memory_fs::MemoryFileSystem;
pub use os::OsSystem;
pub use test::{DbWithTestSystem, TestSystem};

use crate::file_revision::FileRevision;

pub use self::path::{SystemPath, SystemPathBuf};

mod memory_fs;
mod os;
mod path;
mod test;

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
pub trait System {
    /// Reads the metadata of the file or directory at `path`.
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata>;

    /// Reads the content of the file at `path` into a [`String`].
    fn read_to_string(&self, path: &SystemPath) -> Result<String>;

    /// Returns `true` if `path` exists.
    fn path_exists(&self, path: &SystemPath) -> bool {
        self.path_metadata(path).is_ok()
    }

    /// Returns `true` if `path` exists and is a directory.
    fn is_directory(&self, path: &SystemPath) -> bool {
        self.path_metadata(path)
            .map_or(false, |metadata| metadata.file_type.is_directory())
    }

    /// Returns `true` if `path` exists and is a file.
    fn is_file(&self, path: &SystemPath) -> bool {
        self.path_metadata(path)
            .map_or(false, |metadata| metadata.file_type.is_file())
    }

    /// Returns the current working directory
    fn current_directory(&self) -> &SystemPath;

    fn as_any(&self) -> &dyn std::any::Any;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    revision: FileRevision,
    permissions: Option<u32>,
    file_type: FileType,
}

impl Metadata {
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
