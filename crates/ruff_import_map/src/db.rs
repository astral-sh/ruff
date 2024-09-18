use anyhow::Result;
use red_knot_python_semantic::{Db, Program, ProgramSettings, PythonVersion, SearchPathSettings};
use ruff_db::files::{File, Files};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[salsa::db]
pub struct ModuleDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: OsSystem,
    vendored: VendoredFileSystem,
}

impl ModuleDb {
    pub fn from_settings(mut sources: BTreeSet<PathBuf>) -> Result<Self> {
        // TODO(charlie): One database per package root.
        let search_paths = {
            let search_path = sources
                .pop_last()
                .ok_or_else(|| anyhow::anyhow!("No sources provided to module database"))?;
            let mut search_paths = SearchPathSettings::new(
                SystemPathBuf::from_path_buf(search_path)
                    .map_err(|path| anyhow::anyhow!(format!("Invalid path: {}", path.display())))?,
            );
            for source in sources {
                search_paths.extra_paths.push(
                    SystemPathBuf::from_path_buf(source.clone()).map_err(|path| {
                        anyhow::anyhow!(format!("Invalid path: {}", path.display()))
                    })?,
                );
            }
            search_paths
        };

        let db = Self::new();
        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths,
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
