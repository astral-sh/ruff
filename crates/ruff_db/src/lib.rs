use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

use crate::files::Files;
use crate::system::System;
use crate::vendored::VendoredFileSystem;

pub mod diagnostic;
pub mod display;
pub mod file_revision;
pub mod files;
pub mod panic;
pub mod parsed;
pub mod source;
pub mod system;
#[cfg(feature = "testing")]
pub mod testing;
pub mod vendored;

pub type FxDashMap<K, V> = dashmap::DashMap<K, V, BuildHasherDefault<FxHasher>>;
pub type FxDashSet<K> = dashmap::DashSet<K, BuildHasherDefault<FxHasher>>;

/// Most basic database that gives access to files, the host system, source code, and parsed AST.
#[salsa::db]
pub trait Db: salsa::Database {
    fn vendored(&self) -> &VendoredFileSystem;
    fn system(&self) -> &dyn System;
    fn files(&self) -> &Files;
}

/// Trait for upcasting a reference to a base trait object.
pub trait Upcast<T: ?Sized> {
    fn upcast(&self) -> &T;
    fn upcast_mut(&mut self) -> &mut T;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::files::Files;
    use crate::system::TestSystem;
    use crate::system::{DbWithTestSystem, System};
    use crate::vendored::VendoredFileSystem;
    use crate::Db;

    /// Database that can be used for testing.
    ///
    /// Uses an in memory filesystem and it stubs out the vendored files by default.
    #[salsa::db]
    #[derive(Default, Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Arc<std::sync::Mutex<Vec<salsa::Event>>>,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: VendoredFileSystem::default(),
                events: std::sync::Arc::default(),
                files: Files::default(),
            }
        }

        /// Empties the internal store of salsa events that have been emitted,
        /// and returns them as a `Vec` (equivalent to [`std::mem::take`]).
        ///
        /// ## Panics
        /// If there are pending database snapshots.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = Arc::get_mut(&mut self.events)
                .expect("expected no pending salsa database snapshots.");

            std::mem::take(inner.get_mut().unwrap())
        }

        /// Clears the emitted salsa events.
        ///
        /// ## Panics
        /// If there are pending database snapshots.
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_salsa_events();
        }

        pub(crate) fn with_vendored(&mut self, vendored_file_system: VendoredFileSystem) {
            self.vendored = vendored_file_system;
        }
    }

    #[salsa::db]
    impl Db for TestDb {
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
            tracing::trace!("event: {:?}", event);
            let mut events = self.events.lock().unwrap();
            events.push(event);
        }
    }
}
