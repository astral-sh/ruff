use red_knot_python_semantic::{Db, Program, ProgramSettings, PythonVersion, SearchPathSettings};
use ruff_db::files::{File, Files};
use ruff_db::system::SystemPathBuf;
use ruff_db::system::{DbWithTestSystem, System, TestSystem};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};

#[salsa::db]
pub(crate) struct TestDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: TestSystem,
    vendored: VendoredFileSystem,
}

impl TestDb {
    pub(crate) fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            system: TestSystem::default(),
            vendored: red_knot_vendored::file_system().clone(),
            files: Files::default(),
        }
    }

    pub(crate) fn setup(workspace_root: SystemPathBuf) -> Self {
        let db = Self::new();

        db.memory_file_system()
            .create_directory_all(&workspace_root)
            .unwrap();

        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(workspace_root),
            },
        )
        .expect("Valid search path settings");

        db
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
}

#[salsa::db]
impl salsa::Database for TestDb {
    fn salsa_event(&self, _event: &dyn Fn() -> salsa::Event) {}
}
