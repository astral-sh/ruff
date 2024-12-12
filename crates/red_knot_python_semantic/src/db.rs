use crate::lint::RuleSelection;
use ruff_db::files::File;
use ruff_db::{Db as SourceDb, Upcast};

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: SourceDb + Upcast<dyn SourceDb> {
    fn is_file_open(&self, file: File) -> bool;

    fn rule_selection(&self) -> &RuleSelection;
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use crate::program::{Program, SearchPathSettings};
    use crate::python_version::PythonVersion;
    use crate::{default_lint_registry, ProgramSettings};

    use super::Db;
    use crate::lint::RuleSelection;
    use anyhow::Context;
    use ruff_db::files::{File, Files};
    use ruff_db::system::{DbWithTestSystem, System, SystemPathBuf, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Upcast};

    #[salsa::db]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Arc<std::sync::Mutex<Vec<salsa::Event>>>,
        rule_selection: Arc<RuleSelection>,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: red_knot_vendored::file_system().clone(),
                events: Arc::default(),
                files: Files::default(),
                rule_selection: Arc::new(RuleSelection::from_registry(&default_lint_registry())),
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

    #[salsa::db]
    impl Db for TestDb {
        fn is_file_open(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }

        fn rule_selection(&self) -> &RuleSelection {
            &self.rule_selection
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

    pub(crate) struct TestDbBuilder<'a> {
        /// Target Python version
        python_version: PythonVersion,
        /// Path to a custom typeshed directory
        custom_typeshed: Option<SystemPathBuf>,
        /// Path and content pairs for files that should be present
        files: Vec<(&'a str, &'a str)>,
    }

    impl<'a> TestDbBuilder<'a> {
        pub(crate) fn new() -> Self {
            Self {
                python_version: PythonVersion::default(),
                custom_typeshed: None,
                files: vec![],
            }
        }

        pub(crate) fn with_python_version(mut self, version: PythonVersion) -> Self {
            self.python_version = version;
            self
        }

        pub(crate) fn with_custom_typeshed(mut self, path: &str) -> Self {
            self.custom_typeshed = Some(SystemPathBuf::from(path));
            self
        }

        pub(crate) fn with_file(mut self, path: &'a str, content: &'a str) -> Self {
            self.files.push((path, content));
            self
        }

        pub(crate) fn build(self) -> anyhow::Result<TestDb> {
            let mut db = TestDb::new();

            let src_root = SystemPathBuf::from("/src");
            db.memory_file_system().create_directory_all(&src_root)?;

            db.write_files(self.files)
                .context("Failed to write test files")?;

            let mut search_paths = SearchPathSettings::new(src_root);
            search_paths.typeshed = self.custom_typeshed;

            Program::from_settings(
                &db,
                &ProgramSettings {
                    python_version: self.python_version,
                    search_paths,
                },
            )
            .context("Failed to configure Program settings")?;

            Ok(db)
        }
    }

    pub(crate) fn setup_db() -> TestDb {
        TestDbBuilder::new().build().expect("valid TestDb setup")
    }
}
