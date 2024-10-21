use red_knot_python_semantic::{
    Db as SemanticDb, Program, ProgramSettings, PythonVersion, SearchPathSettings,
};
use ruff_db::files::{File, Files};
use ruff_db::system::{DbWithTestSystem, System, SystemPath, SystemPathBuf, TestSystem};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};

#[salsa::db]
pub(crate) struct Db {
    workspace_root: SystemPathBuf,
    storage: salsa::Storage<Self>,
    files: Files,
    system: TestSystem,
    vendored: VendoredFileSystem,
}

impl Db {
    pub(crate) fn setup(workspace_root: SystemPathBuf) -> Self {
        let db = Self {
            workspace_root,
            storage: salsa::Storage::default(),
            system: TestSystem::default(),
            vendored: red_knot_vendored::file_system().clone(),
            files: Files::default(),
        };

        db.memory_file_system()
            .create_directory_all(&db.workspace_root)
            .unwrap();

        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(db.workspace_root.clone()),
            },
        )
        .expect("Invalid search path settings");

        db
    }

    pub(crate) fn workspace_root(&self) -> &SystemPath {
        &self.workspace_root
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
}

#[salsa::db]
impl salsa::Database for Db {
    fn salsa_event(&self, _event: &dyn Fn() -> salsa::Event) {}
}
