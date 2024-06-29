use ruff_db::Upcast;

use crate::resolver::{
    file_to_module,
    internal::{ModuleNameIngredient, ModuleResolverSearchPaths, TargetPyVersion},
    resolve_module_query,
};
use crate::typeshed::TypeshedVersions;

#[salsa::jar(db=Db)]
pub struct Jar(
    ModuleNameIngredient<'_>,
    ModuleResolverSearchPaths,
    TargetPyVersion,
    resolve_module_query,
    file_to_module,
);

pub trait Db: salsa::DbWithJar<Jar> + ruff_db::Db + Upcast<dyn ruff_db::Db> {
    fn typeshed_versions(&self) -> &TypeshedVersions;
}

pub(crate) mod tests {
    use std::str::FromStr;
    use std::sync;

    use salsa::DebugWithDb;

    use ruff_db::file_system::{FileSystem, MemoryFileSystem, OsFileSystem};
    use ruff_db::vfs::Vfs;

    use super::*;

    #[salsa::db(Jar, ruff_db::Jar)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        file_system: TestFileSystem,
        events: sync::Arc<sync::Mutex<Vec<salsa::Event>>>,
        vfs: Vfs,
        typeshed_versions: TypeshedVersions,
    }

    impl TestDb {
        #[allow(unused)]
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                file_system: TestFileSystem::Memory(MemoryFileSystem::default()),
                events: sync::Arc::default(),
                vfs: Vfs::with_stubbed_vendored(),
                typeshed_versions: TypeshedVersions::from_str("").unwrap(),
            }
        }

        /// Returns the memory file system.
        ///
        /// ## Panics
        /// If this test db isn't using a memory file system.
        #[allow(unused)]
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
        #[allow(unused)]
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
        #[allow(unused)]
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = sync::Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

            let events = inner.get_mut().unwrap();
            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        #[allow(unused)]
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

    impl Db for TestDb {
        fn typeshed_versions(&self) -> &TypeshedVersions {
            &self.typeshed_versions
        }
    }

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
                typeshed_versions: self.typeshed_versions.clone(),
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
}
