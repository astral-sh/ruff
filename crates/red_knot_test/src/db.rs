use red_knot_python_semantic::lint::{LintRegistry, RuleSelection};
use red_knot_python_semantic::{
    default_lint_registry, Db as SemanticDb, Program, ProgramSettings, PythonPlatform,
    PythonVersion, SearchPathSettings,
};
use ruff_db::files::{File, Files};
use ruff_db::system::{DbWithTestSystem, System, SystemPath, SystemPathBuf, TestSystem};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};

#[salsa::db]
#[derive(Clone)]
pub(crate) struct Db {
    project_root: SystemPathBuf,
    storage: salsa::Storage<Self>,
    files: Files,
    system: TestSystem,
    vendored: VendoredFileSystem,
    rule_selection: RuleSelection,
}

impl Db {
    pub(crate) fn setup(project_root: SystemPathBuf) -> Self {
        let rule_selection = RuleSelection::from_registry(default_lint_registry());

        let db = Self {
            project_root,
            storage: salsa::Storage::default(),
            system: TestSystem::default(),
            vendored: red_knot_vendored::file_system().clone(),
            files: Files::default(),
            rule_selection,
        };

        db.memory_file_system()
            .create_directory_all(&db.project_root)
            .unwrap();

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersion::default(),
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings::new(vec![db.project_root.clone()]),
            },
        )
        .expect("Invalid search path settings");

        db
    }

    pub(crate) fn project_root(&self) -> &SystemPath {
        &self.project_root
    }
}

impl DbWithTestSystem for Db {
    fn test_system(&self) -> &TestSystem {
        &self.system
    }

    fn test_system_mut(&mut self) -> &mut TestSystem {
        &mut self.system
    }
}

#[salsa::db]
impl SourceDb for Db {
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

impl Upcast<dyn SourceDb> for Db {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }
    fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
        self
    }
}

#[salsa::db]
impl SemanticDb for Db {
    fn is_file_open(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }

    fn rule_selection(&self) -> &RuleSelection {
        &self.rule_selection
    }

    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }
}

#[salsa::db]
impl salsa::Database for Db {
    fn salsa_event(&self, _event: &dyn Fn() -> salsa::Event) {}
}
