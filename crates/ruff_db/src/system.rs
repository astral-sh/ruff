pub use memory_fs::MemoryFileSystem;
pub use os::OsSystem;
use std::any::Any;

use crate::file_revision::FileRevision;

pub use self::path::{SystemPath, SystemPathBuf};

mod memory_fs;
mod os;
mod path;

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

#[derive(Default, Debug)]
pub struct TestSystem {
    inner: TestFileSystem,
}

impl TestSystem {
    pub fn snapshot(&self) -> Self {
        Self {
            inner: self.inner.snapshot(),
        }
    }

    /// Returns the memory file system.
    ///
    /// ## Panics
    /// If this test db isn't using a memory file system.
    pub fn memory_file_system(&self) -> &MemoryFileSystem {
        if let TestFileSystem::Stub(fs) = &self.inner {
            fs
        } else {
            panic!("The test db is not using a memory file system");
        }
    }

    /// Uses the real file system instead of the memory file system.
    ///
    /// This useful for testing advanced file system features like permissions, symlinks, etc.
    ///
    /// Note that any files written to the memory file system won't be copied over.
    pub fn use_os_system(&mut self) {
        self.inner = TestFileSystem::Os(OsSystem);
    }

    /// Writes the content of the given file.
    ///
    /// # Panics
    /// If the host isn't using the memory file system for testing.
    pub fn write_file(&self, path: impl AsRef<SystemPath>, content: impl ToString) -> Result<()> {
        self.memory_file_system().write_file(path, content)
    }

    /// Writes the content of the given file.
    ///
    /// # Panics
    /// If the host isn't using the memory file system for testing.
    pub fn write_files<P, C, I>(&self, files: I) -> Result<()>
    where
        I: IntoIterator<Item = (P, C)>,
        P: AsRef<SystemPath>,
        C: ToString,
    {
        self.memory_file_system().write_files(files)
    }
}

impl System for TestSystem {
    fn metadata(&self, path: &SystemPath) -> Result<Metadata> {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.metadata(path),
            TestFileSystem::Os(fs) => fs.metadata(path),
        }
    }

    fn read_to_string(&self, path: &SystemPath) -> Result<String> {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.read_to_string(path),
            TestFileSystem::Os(fs) => fs.read_to_string(path),
        }
    }

    fn exists(&self, path: &SystemPath) -> bool {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.exists(path),
            TestFileSystem::Os(fs) => fs.exists(path),
        }
    }

    fn is_directory(&self, path: &SystemPath) -> bool {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.is_directory(path),
            TestFileSystem::Os(fs) => fs.is_directory(path),
        }
    }

    fn is_file(&self, path: &SystemPath) -> bool {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.is_file(path),
            TestFileSystem::Os(fs) => fs.is_file(path),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
pub enum TestFileSystem {
    Stub(MemoryFileSystem),
    Os(OsSystem),
}

impl TestFileSystem {
    fn snapshot(&self) -> Self {
        match self {
            Self::Stub(fs) => Self::Stub(fs.snapshot()),
            Self::Os(fs) => Self::Os(fs.snapshot()),
        }
    }
}

impl Default for TestFileSystem {
    fn default() -> Self {
        Self::Stub(MemoryFileSystem::default())
    }
}
