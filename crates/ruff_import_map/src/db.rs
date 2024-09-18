use anyhow::Result;
use red_knot_python_semantic::{Db, Program, ProgramSettings, PythonVersion, SearchPathSettings};
use ruff_db::files::{File, Files};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};
use std::path::PathBuf;

#[salsa::db]
pub struct ModuleDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: OsSystem,
    vendored: VendoredFileSystem,
}

impl Default for ModuleDb {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleDb {
    /// Initialize a [`ModuleDb`] from the given source root.
    pub fn from_src_root(src_root: PathBuf) -> Result<Self> {
        let db = Self::new();
        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(
                    SystemPathBuf::from_path_buf(src_root).map_err(|path| {
                        anyhow::anyhow!(format!("Invalid path: {}", path.display()))
                    })?,
                ),
            },
        )?;
        Ok(db)
    }

    pub fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            system: OsSystem::default(),
            vendored: VendoredFileSystem::default(),
            files: Files::default(),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            system: self.system.clone(),
            vendored: self.vendored.clone(),
            files: self.files.snapshot(),
        }
    }
}

impl Upcast<dyn SourceDb> for ModuleDb {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }
    fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
        self
    }
}

#[salsa::db]
impl SourceDb for ModuleDb {
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

#[salsa::db]
impl Db for ModuleDb {
    fn is_file_open(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }
}

#[salsa::db]
impl salsa::Database for ModuleDb {
    fn salsa_event(&self, _event: &dyn Fn() -> salsa::Event) {}
}
