use crate::module::resolver::{file_to_module, resolve_module_query, ModuleNameIngredient};
use ruff_db::{Db as SourceDb, Upcast};
use salsa::DbWithJar;

#[salsa::jar(db=Db)]
pub struct Jar(ModuleNameIngredient, resolve_module_query, file_to_module);

/// Database giving access to semantic information about a Python program.
pub trait Db: SourceDb + DbWithJar<Jar> + Upcast<dyn SourceDb> {}

#[cfg(test)]
mod tests {
    use super::{Db, Jar};
    use ruff_db::file_system::{FileSystem, MemoryFileSystem};
    use ruff_db::vfs::Vfs;
    use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};
    use salsa::DebugWithDb;

    #[salsa::db(Jar, SourceJar)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        vfs: Vfs,
        file_system: MemoryFileSystem,
        events: std::sync::Arc<std::sync::Mutex<Vec<salsa::Event>>>,
    }

    impl TestDb {
        #[allow(unused)]
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                file_system: MemoryFileSystem::default(),
                events: std::sync::Arc::default(),
                vfs: Vfs::with_stubbed_vendored(),
            }
        }

        #[allow(unused)]
        pub(crate) fn memory_file_system(&self) -> &MemoryFileSystem {
            &self.file_system
        }

        #[allow(unused)]
        pub(crate) fn memory_file_system_mut(&mut self) -> &mut MemoryFileSystem {
            &mut self.file_system
        }

        #[allow(unused)]
        pub(crate) fn vfs_mut(&mut self) -> &mut Vfs {
            &mut self.vfs
        }
    }

    impl SourceDb for TestDb {
        fn file_system(&self) -> &dyn FileSystem {
            &self.file_system
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
                file_system: self.file_system.snapshot(),
                events: self.events.clone(),
            })
        }
    }
}
