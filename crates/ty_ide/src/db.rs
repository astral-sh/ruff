use ruff_db::{Db as SourceDb, Upcast};
use ty_python_semantic::Db as SemanticDb;

#[salsa::db]
pub trait Db: SemanticDb + Upcast<dyn SemanticDb> + Upcast<dyn SourceDb> {}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use super::Db;
    use ruff_db::files::{File, Files};
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Upcast};
    use ty_python_semantic::lint::{LintRegistry, RuleSelection};
    use ty_python_semantic::{default_lint_registry, Db as SemanticDb, Program};

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Arc<std::sync::Mutex<Vec<salsa::Event>>>,
        rule_selection: Arc<RuleSelection>,
    }

    #[expect(dead_code)]
    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: ty_vendored::file_system().clone(),
                events: Arc::default(),
                files: Files::default(),
                rule_selection: Arc::new(RuleSelection::from_registry(default_lint_registry())),
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

        fn python_version(&self) -> ruff_python_ast::PythonVersion {
            Program::get(self).python_version(self)
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

    impl Upcast<dyn SemanticDb> for TestDb {
        fn upcast(&self) -> &(dyn SemanticDb + 'static) {
            self
        }

        fn upcast_mut(&mut self) -> &mut dyn SemanticDb {
            self
        }
    }

    #[salsa::db]
    impl SemanticDb for TestDb {
        fn is_file_open(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }

        fn rule_selection(&self) -> Arc<RuleSelection> {
            self.rule_selection.clone()
        }

        fn lint_registry(&self) -> &LintRegistry {
            default_lint_registry()
        }
    }

    #[salsa::db]
    impl Db for TestDb {}

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
