use ruff_db::Db as SourceDb;
use ruff_db::files::Files;
use ruff_db::system::{DbWithWritableSystem, InMemorySystem, System, SystemPath, WritableSystem};
use ruff_db::vendored::VendoredFileSystem;

#[salsa::db]
#[derive(Clone, Default)]
pub(crate) struct Db {
    storage: salsa::Storage<Self>,
    files: Files,
    system: InMemorySystem,
    vendored: VendoredFileSystem,
}

impl Db {
    pub(crate) fn setup() -> Self {
        Self::default()
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
        &self.vendored
    }

    fn system(&self) -> &dyn System {
        &self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }

    fn python_version(&self) -> ruff_python_ast::PythonVersion {
        ruff_python_ast::PythonVersion::latest()
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
