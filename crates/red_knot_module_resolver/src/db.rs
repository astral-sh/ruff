use ruff_db::Upcast;

#[salsa::db]
pub trait Db: ruff_db::Db + Upcast<dyn ruff_db::Db> {}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync;

    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;

    use crate::vendored_typeshed_stubs;

    use super::*;

    #[salsa::db]
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
                vendored: vendored_typeshed_stubs().clone(),
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
        fn upcast_mut(&mut self) -> &mut (dyn ruff_db::Db + 'static) {
            self
        }
    }

    #[salsa::db]
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

    #[salsa::db]
    impl Db for TestDb {}

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
            let event = event();
            tracing::trace!("event: {event:?}");
            let mut events = self.events.lock().unwrap();
            events.push(event);
        }
    }
}
