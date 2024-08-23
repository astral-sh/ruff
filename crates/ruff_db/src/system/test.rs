use std::any::Any;
use std::panic::RefUnwindSafe;
use std::sync::Arc;

use ruff_notebook::{Notebook, NotebookError};
use ruff_python_trivia::textwrap;

use crate::files::File;
use crate::system::{
    DirectoryEntry, MemoryFileSystem, Metadata, Result, System, SystemPath, SystemPathBuf,
    SystemVirtualPath,
};
use crate::Db;

use super::walk_directory::WalkDirectoryBuilder;

/// System implementation intended for testing.
///
/// It uses a memory-file system by default, but can be switched to the real file system for tests
/// verifying more advanced file system features.
///
/// ## Warning
/// Don't use this system for production code. It's intended for testing only.
#[derive(Default, Debug)]
pub struct TestSystem {
    inner: TestSystemInner,
}

impl TestSystem {
    /// Returns the memory file system.
    ///
    /// ## Panics
    /// If this test db isn't using a memory file system.
    pub fn memory_file_system(&self) -> &MemoryFileSystem {
        if let TestSystemInner::Stub(fs) = &self.inner {
            fs
        } else {
            panic!("The test db is not using a memory file system");
        }
    }

    fn use_system<S>(&mut self, system: S)
    where
        S: System + Send + Sync + RefUnwindSafe + 'static,
    {
        self.inner = TestSystemInner::System(Arc::new(system));
    }
}

impl System for TestSystem {
    fn path_metadata(&self, path: &SystemPath) -> crate::system::Result<Metadata> {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.metadata(path),
            TestSystemInner::System(system) => system.path_metadata(path),
        }
    }

    fn read_to_string(&self, path: &SystemPath) -> crate::system::Result<String> {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.read_to_string(path),
            TestSystemInner::System(system) => system.read_to_string(path),
        }
    }

    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError> {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.read_to_notebook(path),
            TestSystemInner::System(system) => system.read_to_notebook(path),
        }
    }

    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> Result<String> {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.read_virtual_path_to_string(path),
            TestSystemInner::System(system) => system.read_virtual_path_to_string(path),
        }
    }

    fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError> {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.read_virtual_path_to_notebook(path),
            TestSystemInner::System(system) => system.read_virtual_path_to_notebook(path),
        }
    }

    fn path_exists(&self, path: &SystemPath) -> bool {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.exists(path),
            TestSystemInner::System(system) => system.path_exists(path),
        }
    }

    fn is_directory(&self, path: &SystemPath) -> bool {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.is_directory(path),
            TestSystemInner::System(system) => system.is_directory(path),
        }
    }

    fn is_file(&self, path: &SystemPath) -> bool {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.is_file(path),
            TestSystemInner::System(system) => system.is_file(path),
        }
    }

    fn current_directory(&self) -> &SystemPath {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.current_directory(),
            TestSystemInner::System(system) => system.current_directory(),
        }
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        match &self.inner {
            TestSystemInner::Stub(fs) => fs.walk_directory(path),
            TestSystemInner::System(system) => system.walk_directory(path),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>> + 'a>> {
        match &self.inner {
            TestSystemInner::System(fs) => fs.read_directory(path),
            TestSystemInner::Stub(fs) => Ok(Box::new(fs.read_directory(path)?)),
        }
    }

    fn canonicalize_path(&self, path: &SystemPath) -> Result<SystemPathBuf> {
        match &self.inner {
            TestSystemInner::System(fs) => fs.canonicalize_path(path),
            TestSystemInner::Stub(fs) => Ok(fs.canonicalize(path)),
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
            File::sync_path(self, path);
        }

        result
    }

    /// Writes the content of the given virtual file.
    fn write_virtual_file(&mut self, path: impl AsRef<SystemVirtualPath>, content: impl ToString) {
        let path = path.as_ref();
        self.test_system()
            .memory_file_system()
            .write_virtual_file(path, content);
    }

    /// Writes auto-dedented text to a file.
    fn write_dedented(&mut self, path: &str, content: &str) -> crate::system::Result<()> {
        self.write_file(path, textwrap::dedent(content))?;
        Ok(())
    }

    /// Writes the content of the given files and notifies the Db about the change.
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

    /// Uses the given system instead of the testing system.
    ///
    /// This useful for testing advanced file system features like permissions, symlinks, etc.
    ///
    /// Note that any files written to the memory file system won't be copied over.
    fn use_system<S>(&mut self, os: S)
    where
        S: System + Send + Sync + RefUnwindSafe + 'static,
    {
        self.test_system_mut().use_system(os);
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
enum TestSystemInner {
    Stub(MemoryFileSystem),
    System(Arc<dyn System + RefUnwindSafe + Send + Sync>),
}

impl Default for TestSystemInner {
    fn default() -> Self {
        Self::Stub(MemoryFileSystem::default())
    }
}
