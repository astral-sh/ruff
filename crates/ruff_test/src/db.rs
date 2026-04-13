use ruff_db::Db as SourceDb;
use ruff_db::files::{File, Files};
use ruff_db::system::{DbWithWritableSystem, InMemorySystem, System, SystemPath, WritableSystem};
use ruff_db::vendored::VendoredFileSystem;
use ty_module_resolver::SearchPaths;
use ty_python_core::program::Program;

#[salsa::db]
#[derive(Clone)]
pub(crate) struct Db {
    storage: salsa::Storage<Self>,
    files: Files,
    system: InMemorySystem,
}

impl Db {
    pub(crate) fn setup() -> Self {
        Self {
            system: InMemorySystem::default(),
            storage: salsa::Storage::new(Some(Box::new({
                move |event| {
                    tracing::trace!("event: {:?}", event);
                }
            }))),
            files: Files::default(),
        }
    }

    pub(crate) fn use_in_memory_system(&mut self) {
        self.system.fs().remove_all();
        Files::sync_all(self);
    }

    pub(crate) fn create_directory_all(&self, path: &SystemPath) -> ruff_db::system::Result<()> {
        self.system.create_directory_all(path)
    }
}

#[salsa::db]
impl SourceDb for Db {
    fn vendored(&self) -> &VendoredFileSystem {
        ty_vendored::file_system()
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

#[salsa::db]
impl ty_module_resolver::Db for Db {
    fn search_paths(&self) -> &SearchPaths {
        Program::get(self).search_paths(self)
    }
}

#[salsa::db]
impl ty_python_core::Db for Db {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }
}

#[salsa::db]
impl salsa::Database for Db {}

impl DbWithWritableSystem for Db {
    type System = InMemorySystem;
    fn writable_system(&self) -> &Self::System {
        &self.system
    }
}
