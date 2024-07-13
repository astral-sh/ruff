use crate::files::File;
use crate::system::{
    DirectoryEntry, MemoryFileSystem, Metadata, OsSystem, Result, System, SystemPath,
};
use crate::Db;
use std::any::Any;

/// System implementation intended for testing.
///
/// It uses a memory-file system by default, but can be switched to the real file system for tests
/// verifying more advanced file system features.
///
/// ## Warning
/// Don't use this system for production code. It's intended for testing only.
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

    fn use_os_system(&mut self, os: OsSystem) {
        self.inner = TestFileSystem::Os(os);
    }
}

impl System for TestSystem {
    fn path_metadata(&self, path: &SystemPath) -> crate::system::Result<Metadata> {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.metadata(path),
            TestFileSystem::Os(fs) => fs.path_metadata(path),
        }
    }

    fn read_to_string(&self, path: &SystemPath) -> crate::system::Result<String> {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.read_to_string(path),
            TestFileSystem::Os(fs) => fs.read_to_string(path),
        }
    }

    fn path_exists(&self, path: &SystemPath) -> bool {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.exists(path),
            TestFileSystem::Os(fs) => fs.path_exists(path),
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

    fn current_directory(&self) -> &SystemPath {
        match &self.inner {
            TestFileSystem::Stub(fs) => fs.current_directory(),
            TestFileSystem::Os(fs) => fs.current_directory(),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>> + 'a>> {
        match &self.inner {
            TestFileSystem::Os(fs) => fs.read_directory(path),
            TestFileSystem::Stub(fs) => Ok(Box::new(fs.read_directory(path)?)),
        }
    }
}

/// Extension trait for databases that use [`TestSystem`].
///
/// Provides various helper function that ease testing.
pub trait DbWithTestSystem: Db + Sized {
    fn test_system(&self) -> &TestSystem;

    fn test_system_mut(&mut self) -> &mut TestSystem;

    /// Writes the content of the given file and notifies the Db about the change.
    ///
    /// # Panics
    /// If the system isn't using the memory file system.
    fn write_file(
        &mut self,
        path: impl AsRef<SystemPath>,
        content: impl ToString,
    ) -> crate::system::Result<()> {
        let path = path.as_ref();
        let result = self
            .test_system()
            .memory_file_system()
            .write_file(path, content);

        if result.is_ok() {
            File::touch_path(self, path);
        }

        result
    }

    /// Writes the content of the given file and notifies the Db about the change.
    ///
    /// # Panics
    /// If the system isn't using the memory file system for testing.
    fn write_files<P, C, I>(&mut self, files: I) -> crate::system::Result<()>
    where
        I: IntoIterator<Item = (P, C)>,
        P: AsRef<SystemPath>,
        C: ToString,
    {
        for (path, content) in files {
            self.write_file(path, content)?;
        }

        Ok(())
    }

    /// Uses the real file system instead of the memory file system.
    ///
    /// This useful for testing advanced file system features like permissions, symlinks, etc.
    ///
    /// Note that any files written to the memory file system won't be copied over.
    fn use_os_system(&mut self, os: OsSystem) {
        self.test_system_mut().use_os_system(os);
    }

    /// Returns the memory file system.
    ///
    /// ## Panics
    /// If this system isn't using a memory file system.
    fn memory_file_system(&self) -> &MemoryFileSystem {
        self.test_system().memory_file_system()
    }
}

#[derive(Debug)]
enum TestFileSystem {
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
