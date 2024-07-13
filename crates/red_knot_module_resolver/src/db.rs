use ruff_db::Upcast;

use crate::resolver::{
    file_to_module,
    internal::{ModuleNameIngredient, ModuleResolverSettings},
    resolve_module_query,
};
use crate::typeshed::parse_typeshed_versions;

#[salsa::jar(db=Db)]
pub struct Jar(
    ModuleNameIngredient<'_>,
    ModuleResolverSettings,
    resolve_module_query,
    file_to_module,
    parse_typeshed_versions,
);

pub trait Db: salsa::DbWithJar<Jar> + ruff_db::Db + Upcast<dyn ruff_db::Db> {}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync;

    use salsa::DebugWithDb;

    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;

    use crate::vendored_typeshed_stubs;

    use super::*;

    #[salsa::db(Jar, ruff_db::Jar)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        system: TestSystem,
        vendored: VendoredFileSystem,
        files: Files,
        events: sync::Arc<sync::Mutex<Vec<salsa::Event>>>,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: vendored_typeshed_stubs().snapshot(),
                events: sync::Arc::default(),
                files: Files::default(),
            }
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
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn ruff_db::system::System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }
    }

    impl Db for TestDb {}

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
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
                system: self.system.snapshot(),
                vendored: self.vendored.snapshot(),
                files: self.files.snapshot(),
                events: self.events.clone(),
            })
        }
    }
}
