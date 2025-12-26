use crate::AnalysisSettings;
use crate::lint::{LintRegistry, RuleSelection};
use ruff_db::files::File;
use ty_module_resolver::Db as ModuleResolverDb;

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: ModuleResolverDb {
    /// Returns `true` if the file should be checked.
    fn should_check_file(&self, file: File) -> bool;

    /// Resolves the rule selection for a given file.
    fn rule_selection(&self, file: File) -> &RuleSelection;

    fn lint_registry(&self) -> &LintRegistry;

    fn analysis_settings(&self) -> &AnalysisSettings;

    /// Whether ty is running with logging verbosity INFO or higher (`-v` or more).
    fn verbose(&self) -> bool;
}

#[cfg(any(test, feature = "testing"))]
pub mod tests {
    use std::sync::{Arc, Mutex};

    use crate::program::Program;
    use crate::{
        AnalysisSettings, ProgramSettings, PythonPlatform, PythonVersionSource,
        PythonVersionWithSource,
    };
    use ty_module_resolver::SearchPathSettings;

    use super::Db;
    use crate::lint::{LintRegistry, LintRegistryBuilder, RuleSelection};
    use anyhow::Context;
    use ruff_db::Db as SourceDb;
    use ruff_db::files::{File, Files};
    use ruff_db::system::{
        DbWithTestSystem, DbWithWritableSystem as _, System, SystemPath, SystemPathBuf, TestSystem,
    };
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_python_ast::PythonVersion;
    use ty_module_resolver::Db as ModuleResolverDb;
    use ty_module_resolver::SearchPaths;

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    /// A default lint registry containing only the suppression lints.
    /// Full lint registry should be obtained from `ty_python_types`.
    fn test_lint_registry() -> &'static LintRegistry {
        static REGISTRY: std::sync::LazyLock<LintRegistry> = std::sync::LazyLock::new(|| {
            let mut builder = LintRegistryBuilder::default();
            crate::register_suppression_lints(&mut builder);
            builder.build()
        });
        &REGISTRY
    }

    #[salsa::db]
    #[derive(Clone)]
    pub struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Events,
        rule_selection: Arc<RuleSelection>,
        lint_registry: Arc<LintRegistry>,
        analysis_settings: Arc<AnalysisSettings>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            Self::new()
        }
    }

    impl TestDb {
        pub fn new() -> Self {
            Self::with_vendored(ty_vendored::file_system().clone())
        }

        pub fn with_vendored(vendored: VendoredFileSystem) -> Self {
            let events = Events::default();
            let lint_registry = test_lint_registry();
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
                vendored,
                events,
                files: Files::default(),
                rule_selection: Arc::new(RuleSelection::from_registry(lint_registry)),
                lint_registry: Arc::new(lint_registry.clone()),
                analysis_settings: AnalysisSettings::default().into(),
            }
        }

        /// Takes the salsa events.
        pub fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let mut events = self.events.lock().unwrap();

            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub fn clear_salsa_events(&mut self) {
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

        fn python_version(&self) -> PythonVersion {
            Program::get(self).python_version(self)
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn should_check_file(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }

        fn rule_selection(&self, _file: File) -> &RuleSelection {
            &self.rule_selection
        }

        fn lint_registry(&self) -> &LintRegistry {
            &self.lint_registry
        }

        fn analysis_settings(&self) -> &AnalysisSettings {
            &self.analysis_settings
        }

        fn verbose(&self) -> bool {
            false
        }
    }

    #[salsa::db]
    impl ModuleResolverDb for TestDb {
        fn search_paths(&self) -> &SearchPaths {
            Program::get(self).search_paths(self)
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    pub struct TestDbBuilder<'a> {
        /// Target Python version
        python_version: PythonVersion,
        /// Target Python platform
        python_platform: PythonPlatform,
        /// Path and content pairs for files that should be present
        files: Vec<(&'a str, &'a str)>,
    }

    impl Default for TestDbBuilder<'_> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<'a> TestDbBuilder<'a> {
        pub fn new() -> Self {
            Self {
                python_version: PythonVersion::default(),
                python_platform: PythonPlatform::default(),
                files: vec![],
            }
        }

        #[must_use]
        pub fn with_python_version(mut self, version: PythonVersion) -> Self {
            self.python_version = version;
            self
        }

        #[must_use]
        pub fn with_file(
            mut self,
            path: &'a (impl AsRef<SystemPath> + ?Sized),
            content: &'a str,
        ) -> Self {
            self.files.push((path.as_ref().as_str(), content));
            self
        }

        pub fn build(self) -> anyhow::Result<TestDb> {
            let mut db = TestDb::new();

            let src_root = SystemPathBuf::from("/src");
            db.memory_file_system().create_directory_all(&src_root)?;

            db.write_files(self.files)
                .context("Failed to write test files")?;

            Program::from_settings(
                &db,
                ProgramSettings {
                    python_version: PythonVersionWithSource {
                        version: self.python_version,
                        source: PythonVersionSource::default(),
                    },
                    python_platform: self.python_platform,
                    search_paths: SearchPathSettings::new(vec![src_root])
                        .to_search_paths(db.system(), db.vendored())
                        .context("Invalid search path settings")?,
                },
            );

            Ok(db)
        }
    }

    pub fn setup_db() -> TestDb {
        TestDbBuilder::new().build().expect("valid TestDb setup")
    }
}
