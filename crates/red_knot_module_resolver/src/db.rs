use ruff_db::Upcast;

use crate::resolver::{
    file_to_module,
    internal::{ModuleNameIngredient, ModuleResolverSearchPaths},
    resolve_module_query,
};
use crate::supported_py_version::TargetPyVersion;
use crate::typeshed::parse_typeshed_versions;

#[salsa::jar(db=Db)]
pub struct Jar(
    ModuleNameIngredient<'_>,
    ModuleResolverSearchPaths,
    TargetPyVersion,
    resolve_module_query,
    file_to_module,
    parse_typeshed_versions,
);

pub trait Db: salsa::DbWithJar<Jar> + ruff_db::Db + Upcast<dyn ruff_db::Db> {}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync;

    use salsa::DebugWithDb;

    use ruff_db::file_system::{
        FileSystem, FileSystemPath, FileSystemPathBuf, MemoryFileSystem, OsFileSystem,
    };
    use ruff_db::vfs::Vfs;

    use crate::resolver::{set_module_resolution_settings, ModuleResolutionSettings};
    use crate::supported_py_version::SupportedPyVersion;

    use super::*;

    #[salsa::db(Jar, ruff_db::Jar)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        file_system: TestFileSystem,
        events: sync::Arc<sync::Mutex<Vec<salsa::Event>>>,
        vfs: Vfs,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                file_system: TestFileSystem::Memory(MemoryFileSystem::default()),
                events: sync::Arc::default(),
                vfs: Vfs::with_stubbed_vendored(),
            }
        }

        /// Returns the memory file system.
        ///
        /// ## Panics
        /// If this test db isn't using a memory file system.
        pub(crate) fn memory_file_system(&self) -> &MemoryFileSystem {
            if let TestFileSystem::Memory(fs) = &self.file_system {
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
        pub(crate) fn with_os_file_system(&mut self) {
            self.file_system = TestFileSystem::Os(OsFileSystem);
        }

        #[allow(unused)]
        pub(crate) fn vfs_mut(&mut self) -> &mut Vfs {
            &mut self.vfs
        }

        /// Takes the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = sync::Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

            let events = inner.get_mut().unwrap();
            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_salsa_events();
        }
    }

    impl Upcast<dyn ruff_db::Db> for TestDb {
        fn upcast(&self) -> &(dyn ruff_db::Db + 'static) {
            self
        }
    }

    impl ruff_db::Db for TestDb {
        fn file_system(&self) -> &dyn ruff_db::file_system::FileSystem {
            self.file_system.inner()
        }

        fn vfs(&self) -> &ruff_db::vfs::Vfs {
            &self.vfs
        }
    }

    impl Db for TestDb {}

    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: salsa::Event) {
            tracing::trace!("event: {:?}", event.debug(self));
            let mut events = self.events.lock().unwrap();
            events.push(event);
        }
    }

    impl salsa::ParallelDatabase for TestDb {
        fn snapshot(&self) -> salsa::Snapshot<Self> {
            salsa::Snapshot::new(Self {
                storage: self.storage.snapshot(),
                file_system: self.file_system.snapshot(),
                events: self.events.clone(),
                vfs: self.vfs.snapshot(),
            })
        }
    }

    enum TestFileSystem {
        Memory(MemoryFileSystem),
        #[allow(unused)]
        Os(OsFileSystem),
    }

    impl TestFileSystem {
        fn inner(&self) -> &dyn FileSystem {
            match self {
                Self::Memory(inner) => inner,
                Self::Os(inner) => inner,
            }
        }

        fn snapshot(&self) -> Self {
            match self {
                Self::Memory(inner) => Self::Memory(inner.snapshot()),
                Self::Os(inner) => Self::Os(inner.snapshot()),
            }
        }
    }

    pub(crate) struct TestCaseBuilder {
        db: TestDb,
        src: FileSystemPathBuf,
        custom_typeshed: FileSystemPathBuf,
        site_packages: FileSystemPathBuf,
        target_version: Option<SupportedPyVersion>,
    }

    impl TestCaseBuilder {
        #[must_use]
        pub(crate) fn with_target_version(mut self, target_version: SupportedPyVersion) -> Self {
            self.target_version = Some(target_version);
            self
        }

        pub(crate) fn build(self) -> TestCase {
            let TestCaseBuilder {
                mut db,
                src,
                custom_typeshed,
                site_packages,
                target_version,
            } = self;

            let settings = ModuleResolutionSettings {
                target_version: target_version.unwrap_or_default(),
                extra_paths: vec![],
                workspace_root: src.clone(),
                custom_typeshed: Some(custom_typeshed.clone()),
                site_packages: Some(site_packages.clone()),
            };

            set_module_resolution_settings(&mut db, settings);

            TestCase {
                db,
                src,
                custom_typeshed,
                site_packages,
            }
        }
    }

    pub(crate) struct TestCase {
        pub(crate) db: TestDb,
        pub(crate) src: FileSystemPathBuf,
        pub(crate) custom_typeshed: FileSystemPathBuf,
        pub(crate) site_packages: FileSystemPathBuf,
    }

    pub(crate) fn create_resolver_builder() -> std::io::Result<TestCaseBuilder> {
        static VERSIONS_DATA: &str = "\
        asyncio: 3.8-               # 'Regular' package on py38+
        asyncio.tasks: 3.9-3.11
        collections: 3.9-           # 'Regular' package on py39+
        functools: 3.8-
        importlib: 3.9-             # Namespace package on py39+
        xml: 3.8-3.8                # Namespace package on py38 only
        ";

        let db = TestDb::new();

        let src = FileSystemPath::new("src").to_path_buf();
        let site_packages = FileSystemPath::new("site_packages").to_path_buf();
        let custom_typeshed = FileSystemPath::new("typeshed").to_path_buf();

        let fs = db.memory_file_system();

        fs.create_directory_all(&*src)?;
        fs.create_directory_all(&*site_packages)?;
        fs.create_directory_all(&*custom_typeshed)?;
        fs.write_file(custom_typeshed.join("stdlib/VERSIONS"), VERSIONS_DATA)?;

        // Regular package on py38+
        fs.create_directory_all(custom_typeshed.join("stdlib/asyncio"))?;
        fs.touch(custom_typeshed.join("stdlib/asyncio/__init__.pyi"))?;
        fs.write_file(
            custom_typeshed.join("stdlib/asyncio/tasks.pyi"),
            "class Task: ...",
        )?;

        // Regular package on py39+
        fs.create_directory_all(custom_typeshed.join("stdlib/collections"))?;
        fs.touch(custom_typeshed.join("stdlib/collections/__init__.pyi"))?;

        // Namespace package on py38 only
        fs.create_directory_all(custom_typeshed.join("stdlib/xml"))?;
        fs.touch(custom_typeshed.join("stdlib/xml/etree.pyi"))?;

        // Namespace package on py39+
        fs.create_directory_all(custom_typeshed.join("stdlib/importlib"))?;
        fs.write_file(
            custom_typeshed.join("stdlib/functools.pyi"),
            "def update_wrapper(): ...",
        )?;

        Ok(TestCaseBuilder {
            db,
            src,
            custom_typeshed,
            site_packages,
            target_version: None,
        })
    }
}
