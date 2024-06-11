use crate::module::resolver::{
    file_to_module, internal::ModuleNameIngredient, internal::ModuleResolverSearchPaths,
    resolve_module_query,
};
use ruff_db::{Db as SourceDb, Upcast};
use salsa::DbWithJar;

#[salsa::jar(db=Db)]
pub struct Jar(
    ModuleNameIngredient,
    ModuleResolverSearchPaths,
    resolve_module_query,
    file_to_module,
);

/// Database giving access to semantic information about a Python program.
pub trait Db: SourceDb + DbWithJar<Jar> + Upcast<dyn SourceDb> {}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Db, Jar};
    use ruff_db::file_system::{FileSystem, MemoryFileSystem, OsFileSystem};
    use ruff_db::vfs::Vfs;
    use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};
    use salsa::DebugWithDb;
    use std::sync::Arc;

    #[salsa::db(Jar, SourceJar)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        vfs: Vfs,
        file_system: TestFileSystem,
        events: std::sync::Arc<std::sync::Mutex<Vec<salsa::Event>>>,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                file_system: TestFileSystem::Memory(MemoryFileSystem::default()),
                events: std::sync::Arc::default(),
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
        pub(crate) fn take_sale_events(&mut self) -> Vec<salsa::Event> {
            let inner = Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

            let events = inner.get_mut().unwrap();
            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_sale_events();
        }
    }

    impl SourceDb for TestDb {
        fn file_system(&self) -> &dyn FileSystem {
            match &self.file_system {
                TestFileSystem::Memory(fs) => fs,
                TestFileSystem::Os(fs) => fs,
            }
        }

        fn vfs(&self) -> &Vfs {
            &self.vfs
        }
    }

    impl Upcast<dyn SourceDb> for TestDb {
        fn upcast(&self) -> &(dyn SourceDb + 'static) {
            self
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
                vfs: self.vfs.snapshot(),
                file_system: match &self.file_system {
                    TestFileSystem::Memory(memory) => TestFileSystem::Memory(memory.snapshot()),
                    TestFileSystem::Os(fs) => TestFileSystem::Os(fs.snapshot()),
                },
                events: self.events.clone(),
            })
        }
    }

    enum TestFileSystem {
        Memory(MemoryFileSystem),
        Os(OsFileSystem),
    }
}
