use ruff_db::Db as SourceDb;

use crate::resolve::SearchPaths;

#[salsa::db]
pub trait Db: SourceDb {
    /// Returns the search paths for module resolution.
    fn search_paths(&self) -> &SearchPaths;
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use ruff_db::Db as SourceDb;
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_python_ast::PythonVersion;

    use super::Db;
    use crate::resolve::SearchPaths;

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        search_paths: Arc<SearchPaths>,
        python_version: PythonVersion,
        events: Events,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            let events = Events::default();
            Self {
                storage: salsa::Storage::new(Some(Box::new({
                    let events = events.clone();
                    move |event| {
                        tracing::trace!("event: {event:?}");
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored: ty_vendored::file_system().clone(),
                files: Files::default(),
                search_paths: Arc::new(SearchPaths::empty(ty_vendored::file_system())),
                python_version: PythonVersion::default(),
                events,
            }
        }

        pub(crate) fn with_search_paths(mut self, search_paths: SearchPaths) -> Self {
            self.set_search_paths(search_paths);
            self
        }

        pub(crate) fn with_python_version(mut self, python_version: PythonVersion) -> Self {
            self.python_version = python_version;
            self
        }

        pub(crate) fn set_search_paths(&mut self, search_paths: SearchPaths) {
            search_paths.try_register_static_roots(self);
            self.search_paths = Arc::new(search_paths);
        }

        /// Takes the salsa events.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let mut events = self.events.lock().unwrap();
            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
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

        fn system(&self) -> &dyn ruff_db::system::System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }

        fn python_version(&self) -> PythonVersion {
            self.python_version
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn search_paths(&self) -> &SearchPaths {
            &self.search_paths
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}
}
