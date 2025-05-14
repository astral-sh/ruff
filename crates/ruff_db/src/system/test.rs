use glob::PatternError;
use ruff_notebook::{Notebook, NotebookError};
use std::panic::RefUnwindSafe;
use std::sync::{Arc, Mutex};

use crate::files::File;
use crate::system::{
    CaseSensitivity, DirectoryEntry, GlobError, MemoryFileSystem, Metadata, Result, System,
    SystemPath, SystemPathBuf, SystemVirtualPath,
};
use crate::Db;

use super::walk_directory::WalkDirectoryBuilder;
use super::WritableSystem;

/// System implementation intended for testing.
///
/// It uses a memory-file system by default, but can be switched to the real file system for tests
/// verifying more advanced file system features.
///
/// ## Warning
/// Don't use this system for production code. It's intended for testing only.
#[derive(Debug, Clone)]
pub struct TestSystem {
    inner: Arc<dyn WritableSystem + RefUnwindSafe + Send + Sync>,
}

impl TestSystem {
    pub fn new(inner: impl WritableSystem + RefUnwindSafe + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Returns the [`InMemorySystem`].
    ///
    /// ## Panics
    /// If the underlying test system isn't the [`InMemorySystem`].
    pub fn in_memory(&self) -> &InMemorySystem {
        self.as_in_memory()
            .expect("The test db is not using a memory file system")
    }

    /// Returns the `InMemorySystem` or `None` if the underlying test system isn't the [`InMemorySystem`].
    pub fn as_in_memory(&self) -> Option<&InMemorySystem> {
        self.system().as_any().downcast_ref::<InMemorySystem>()
    }

    /// Returns the memory file system.
    ///
    /// ## Panics
    /// If the underlying test system isn't the [`InMemorySystem`].
    pub fn memory_file_system(&self) -> &MemoryFileSystem {
        self.in_memory().fs()
    }

    fn use_system<S>(&mut self, system: S)
    where
        S: WritableSystem + Send + Sync + RefUnwindSafe + 'static,
    {
        self.inner = Arc::new(system);
    }

    pub fn system(&self) -> &dyn WritableSystem {
        &*self.inner
    }
}

impl System for TestSystem {
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata> {
        self.system().path_metadata(path)
    }

    fn canonicalize_path(&self, path: &SystemPath) -> Result<SystemPathBuf> {
        self.system().canonicalize_path(path)
    }

    fn read_to_string(&self, path: &SystemPath) -> Result<String> {
        self.system().read_to_string(path)
    }

    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError> {
        self.system().read_to_notebook(path)
    }

    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> Result<String> {
        self.system().read_virtual_path_to_string(path)
    }

    fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError> {
        self.system().read_virtual_path_to_notebook(path)
    }

    fn current_directory(&self) -> &SystemPath {
        self.system().current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        self.system().user_config_directory()
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>> + 'a>> {
        self.system().read_directory(path)
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        self.system().walk_directory(path)
    }

    fn glob(
        &self,
        pattern: &str,
    ) -> std::result::Result<
        Box<dyn Iterator<Item = std::result::Result<SystemPathBuf, GlobError>>>,
        PatternError,
    > {
        self.system().glob(pattern)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn path_exists_case_sensitive(&self, path: &SystemPath, prefix: &SystemPath) -> bool {
        self.system().path_exists_case_sensitive(path, prefix)
    }

    fn case_sensitivity(&self) -> CaseSensitivity {
        self.system().case_sensitivity()
    }
}

impl Default for TestSystem {
    fn default() -> Self {
        Self {
            inner: Arc::new(InMemorySystem::default()),
        }
    }
}

impl WritableSystem for TestSystem {
    fn write_file(&self, path: &SystemPath, content: &str) -> Result<()> {
        self.system().write_file(path, content)
    }

    fn create_directory_all(&self, path: &SystemPath) -> Result<()> {
        self.system().create_directory_all(path)
    }
}

/// Extension trait for databases that use a [`WritableSystem`].
///
/// Provides various helper function that ease testing.
pub trait DbWithWritableSystem: Db + Sized {
    type System: WritableSystem;

    fn writable_system(&self) -> &Self::System;

    /// Writes the content of the given file and notifies the Db about the change.
    fn write_file(&mut self, path: impl AsRef<SystemPath>, content: impl AsRef<str>) -> Result<()> {
        let path = path.as_ref();
        match self.writable_system().write_file(path, content.as_ref()) {
            Ok(()) => {
                File::sync_path(self, path);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(parent) = path.parent() {
                    self.writable_system().create_directory_all(parent)?;

                    for ancestor in parent.ancestors() {
                        File::sync_path(self, ancestor);
                    }

                    self.writable_system().write_file(path, content.as_ref())?;
                    File::sync_path(self, path);

                    Ok(())
                } else {
                    Err(error)
                }
            }
            err => err,
        }
    }

    /// Writes auto-dedented text to a file.
    fn write_dedented(&mut self, path: &str, content: &str) -> Result<()> {
        self.write_file(path, ruff_python_trivia::textwrap::dedent(content))?;
        Ok(())
    }

    /// Writes the content of the given files and notifies the Db about the change.
    fn write_files<P, C, I>(&mut self, files: I) -> Result<()>
    where
        I: IntoIterator<Item = (P, C)>,
        P: AsRef<SystemPath>,
        C: AsRef<str>,
    {
        for (path, content) in files {
            self.write_file(path, content)?;
        }

        Ok(())
    }
}

/// Extension trait for databases that use [`TestSystem`].
///
/// Provides various helper function that ease testing.
pub trait DbWithTestSystem: Db + Sized {
    fn test_system(&self) -> &TestSystem;

    fn test_system_mut(&mut self) -> &mut TestSystem;

    /// Writes the content of the given virtual file.
    ///
    /// ## Panics
    /// If the db isn't using the [`InMemorySystem`].
    fn write_virtual_file(&mut self, path: impl AsRef<SystemVirtualPath>, content: impl ToString) {
        let path = path.as_ref();
        self.test_system()
            .memory_file_system()
            .write_virtual_file(path, content);
    }

    /// Uses the given system instead of the testing system.
    ///
    /// This useful for testing advanced file system features like permissions, symlinks, etc.
    ///
    /// Note that any files written to the memory file system won't be copied over.
    fn use_system<S>(&mut self, os: S)
    where
        S: WritableSystem + Send + Sync + RefUnwindSafe + 'static,
    {
        self.test_system_mut().use_system(os);
    }

    /// Returns the memory file system.
    ///
    /// ## Panics
    /// If the underlying test system isn't the [`InMemorySystem`].
    fn memory_file_system(&self) -> &MemoryFileSystem {
        self.test_system().memory_file_system()
    }
}

impl<T> DbWithWritableSystem for T
where
    T: DbWithTestSystem,
{
    type System = TestSystem;

    fn writable_system(&self) -> &Self::System {
        self.test_system()
    }
}

#[derive(Default, Debug)]
pub struct InMemorySystem {
    user_config_directory: Mutex<Option<SystemPathBuf>>,
    memory_fs: MemoryFileSystem,
}

impl InMemorySystem {
    pub fn new(cwd: SystemPathBuf) -> Self {
        Self {
            user_config_directory: Mutex::new(None),
            memory_fs: MemoryFileSystem::with_current_directory(cwd),
        }
    }

    pub fn fs(&self) -> &MemoryFileSystem {
        &self.memory_fs
    }

    pub fn set_user_configuration_directory(&self, directory: Option<SystemPathBuf>) {
        let mut user_directory = self.user_config_directory.lock().unwrap();
        *user_directory = directory;
    }
}

impl System for InMemorySystem {
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata> {
        self.memory_fs.metadata(path)
    }

    fn canonicalize_path(&self, path: &SystemPath) -> Result<SystemPathBuf> {
        self.memory_fs.canonicalize(path)
    }

    fn read_to_string(&self, path: &SystemPath) -> Result<String> {
        self.memory_fs.read_to_string(path)
    }

    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError> {
        let content = self.read_to_string(path)?;
        Notebook::from_source_code(&content)
    }

    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> Result<String> {
        self.memory_fs.read_virtual_path_to_string(path)
    }

    fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError> {
        let content = self.read_virtual_path_to_string(path)?;
        Notebook::from_source_code(&content)
    }

    fn current_directory(&self) -> &SystemPath {
        self.memory_fs.current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        self.user_config_directory.lock().unwrap().clone()
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>> + 'a>> {
        Ok(Box::new(self.memory_fs.read_directory(path)?))
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        self.memory_fs.walk_directory(path)
    }

    fn glob(
        &self,
        pattern: &str,
    ) -> std::result::Result<
        Box<dyn Iterator<Item = std::result::Result<SystemPathBuf, GlobError>>>,
        PatternError,
    > {
        let iterator = self.memory_fs.glob(pattern)?;
        Ok(Box::new(iterator))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    #[inline]
    fn path_exists_case_sensitive(&self, path: &SystemPath, _prefix: &SystemPath) -> bool {
        // The memory file system is case-sensitive.
        self.path_exists(path)
    }

    fn case_sensitivity(&self) -> CaseSensitivity {
        CaseSensitivity::CaseSensitive
    }
}

impl WritableSystem for InMemorySystem {
    fn write_file(&self, path: &SystemPath, content: &str) -> Result<()> {
        self.memory_fs.write_file(path, content)
    }

    fn create_directory_all(&self, path: &SystemPath) -> Result<()> {
        self.memory_fs.create_directory_all(path)
    }
}
