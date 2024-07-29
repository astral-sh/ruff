use red_knot_module_resolver::Db as ResolverDb;
use ruff_db::Upcast;

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: ResolverDb + Upcast<dyn ResolverDb> {}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use red_knot_module_resolver::{vendored_typeshed_stubs, Db as ResolverDb};
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Upcast};

    use super::Db;

    #[salsa::db]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: std::sync::Arc<std::sync::Mutex<Vec<salsa::Event>>>,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: vendored_typeshed_stubs().clone(),
                events: std::sync::Arc::default(),
                files: Files::default(),
            }
        }

        /// Takes the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

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

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    #[salsa::db]
    impl SourceDb for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }
    }

    impl Upcast<dyn SourceDb> for TestDb {
        fn upcast(&self) -> &(dyn SourceDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
            self
        }
    }

    impl Upcast<dyn ResolverDb> for TestDb {
        fn upcast(&self) -> &(dyn ResolverDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn ResolverDb + 'static) {
            self
        }
    }

    #[salsa::db]
    impl red_knot_module_resolver::Db for TestDb {}

    #[salsa::db]
    impl Db for TestDb {}

    #[salsa::db]
    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: salsa::Event) {
            self.attach(|_| {
                tracing::trace!("event: {event:?}");
                let mut events = self.events.lock().unwrap();
                events.push(event);
            });
        }
    }
}
