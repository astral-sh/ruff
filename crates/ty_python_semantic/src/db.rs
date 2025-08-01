use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::lint::{LintRegistry, RuleSelection};
use crate::semantic_index::scope::FileScopeId;
use ruff_db::Db as SourceDb;
use ruff_db::files::File;
use rustc_hash::FxHasher;

/// A stack of the currently inferred function scopes.
/// Used to monitor type inference for recursive functions to ensure they do not diverge.
/// This call stack is currently only used to infer functions with unspecified return types ​​and does not faithfully represent the actual call stack.
#[derive(Debug, Default, Clone)]
pub struct CallStack(Arc<Mutex<Vec<(File, FileScopeId)>>>);

impl CallStack {
    pub fn new() -> Self {
        CallStack(Arc::new(Mutex::new(Vec::new())))
    }

    pub fn push(&self, file: File, scope: FileScopeId) {
        self.0.lock().unwrap().push((file, scope));
    }

    pub fn pop(&self) -> Option<(File, FileScopeId)> {
        self.0.lock().unwrap().pop()
    }

    pub fn contains(&self, file: File, scope: FileScopeId) -> bool {
        self.0
            .lock()
            .unwrap()
            .iter()
            .any(|(f, s)| f == &file && s == &scope)
    }

    pub fn hash_value(&self) -> u64 {
        let mut hasher = FxHasher::default();
        self.0.lock().unwrap().hash(&mut hasher);
        hasher.finish()
    }
}

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: SourceDb {
    /// Returns `true` if the file should be checked.
    fn should_check_file(&self, file: File) -> bool;

    /// Resolves the rule selection for a given file.
    fn rule_selection(&self, file: File) -> &RuleSelection;

    fn lint_registry(&self) -> &LintRegistry;

    fn call_stack(&self) -> &CallStack;
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use crate::program::{Program, SearchPathSettings};
    use crate::{
        ProgramSettings, PythonPlatform, PythonVersionSource, PythonVersionWithSource,
        default_lint_registry,
    };

    use super::{CallStack, Db};
    use crate::lint::{LintRegistry, RuleSelection};
    use anyhow::Context;
    use ruff_db::Db as SourceDb;
    use ruff_db::files::{File, Files};
    use ruff_db::system::{
        DbWithTestSystem, DbWithWritableSystem as _, System, SystemPath, SystemPathBuf, TestSystem,
    };
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_python_ast::PythonVersion;

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Events,
        rule_selection: Arc<RuleSelection>,
        call_stack: CallStack,
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
                events,
                files: Files::default(),
                rule_selection: Arc::new(RuleSelection::from_registry(default_lint_registry())),
                call_stack: CallStack::default(),
            }
        }

        /// Takes the salsa events.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let mut events = self.events.lock().unwrap();

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
            default_lint_registry()
        }

        fn call_stack(&self) -> &CallStack {
            &self.call_stack
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    pub(crate) struct TestDbBuilder<'a> {
        /// Target Python version
        python_version: PythonVersion,
        /// Target Python platform
        python_platform: PythonPlatform,
        /// Path and content pairs for files that should be present
        files: Vec<(&'a str, &'a str)>,
    }

    impl<'a> TestDbBuilder<'a> {
        pub(crate) fn new() -> Self {
            Self {
                python_version: PythonVersion::default(),
                python_platform: PythonPlatform::default(),
                files: vec![],
            }
        }

        pub(crate) fn with_python_version(mut self, version: PythonVersion) -> Self {
            self.python_version = version;
            self
        }

        pub(crate) fn with_file(
            mut self,
            path: &'a (impl AsRef<SystemPath> + ?Sized),
            content: &'a str,
        ) -> Self {
            self.files.push((path.as_ref().as_str(), content));
            self
        }

        pub(crate) fn build(self) -> anyhow::Result<TestDb> {
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

    pub(crate) fn setup_db() -> TestDb {
        TestDbBuilder::new().build().expect("valid TestDb setup")
    }
}
