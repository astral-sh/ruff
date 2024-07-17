use std::fmt;
use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;
use salsa::DbWithJar;

use crate::files::{File, Files};
use crate::parsed::parsed_module;
use crate::source::{line_index, source_text};
use crate::system::System;
use crate::vendored::VendoredFileSystem;

pub mod file_revision;
pub mod files;
pub mod parsed;
pub mod source;
pub mod system;
pub mod testing;
pub mod vendored;

pub(crate) type FxDashMap<K, V> = dashmap::DashMap<K, V, BuildHasherDefault<FxHasher>>;

#[salsa::jar(db=Db)]
pub struct Jar(File, source_text, line_index, parsed_module);

/// Most basic database that gives access to files, the host system, source code, and parsed AST.
pub trait Db: DbWithJar<Jar> + fmt::Debug {
    fn vendored(&self) -> &VendoredFileSystem;
    fn system(&self) -> &dyn System;
    fn files(&self) -> &Files;
}

/// Trait for upcasting a reference to a base trait object.
pub trait Upcast<T: ?Sized> {
    fn upcast(&self) -> &T;
}

#[cfg(test)]
mod tests {
    use std::fmt;
    use std::sync::Arc;

    use insta::assert_snapshot;
    use salsa::DebugWithDb;

    use crate::files::Files;
    use crate::system::TestSystem;
    use crate::system::{DbWithTestSystem, System};
    use crate::vendored::VendoredFileSystem;
    use crate::{Db, Jar};

    /// Database that can be used for testing.
    ///
    /// Uses an in memory filesystem and it stubs out the vendored files by default.
    #[derive(Default)]
    #[salsa::db(Jar)]
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
        #[allow(unused)]
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = Arc::get_mut(&mut self.events)
                .expect("expected no pending salsa database snapshots.");

            std::mem::take(inner.get_mut().unwrap())
        }

        /// Clears the emitted salsa events.
        ///
        /// ## Panics
        /// If there are pending database snapshots.
        #[allow(unused)]
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_salsa_events();
        }

        pub(crate) fn with_vendored(&mut self, vendored_file_system: VendoredFileSystem) {
            self.vendored = vendored_file_system;
        }
    }

    impl fmt::Debug for TestDb {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let TestDb {
                storage: _,
                files: _,
                system,
                vendored,
                events,
            } = self;
            let num_events = events.lock().unwrap().len();
            f.debug_struct("TestDb")
                .field("system", system)
                .field("total_salsa_events", &num_events)
                .field("vendored", vendored)
                .finish()
        }
    }

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
                files: self.files.snapshot(),
                events: self.events.clone(),
                vendored: self.vendored.snapshot(),
            })
        }
    }

    #[test]
    fn test_db_debug_impl() {
        assert_snapshot!(
            format!("{:?}", TestDb::new()),
            @"TestDb { system: TestSystem { inner: TestFileSystem::Stub(...) }, total_salsa_events: 0, vendored: VendoredFileSystem(<0 paths>) }"
        );
    }
}
