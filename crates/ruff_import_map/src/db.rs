use crate::ImportMapSettings;
use anyhow::Result;
use red_knot_python_semantic::{
    Db, Module, ModuleName, Program, ProgramSettings, PythonVersion, SearchPathSettings,
};
use ruff_db::files::{File, Files};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};
use std::path::PathBuf;
use std::sync::Arc;

#[salsa::db]
pub struct ModuleDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: OsSystem,
    vendored: VendoredFileSystem,
    events: Arc<std::sync::Mutex<Vec<salsa::Event>>>,
}

impl ModuleDb {
    pub fn from_settings(settings: &ImportMapSettings) -> Result<Self> {
        let db = Self::new();
        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(
                    SystemPathBuf::from_path_buf(settings.src[0].clone())
                        .expect("Invalid search path"),
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
            events: Arc::default(),
            files: Files::default(),
        }
    }

    pub(crate) fn resolve<'path>(
        db: &ModuleDb,
        module_name: &'path [&'path str],
    ) -> Option<Module> {
        let module_name = ModuleName::from_components(module_name.iter().copied())?;
        red_knot_python_semantic::resolve_module(db, module_name)
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
    fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
        let event = event();

        let mut events = self.events.lock().unwrap();
        events.push(event);
    }
}
