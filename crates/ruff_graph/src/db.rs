use anyhow::{Context, Result};
use std::sync::Arc;
use zip::CompressionMethod;

use ruff_db::Db as SourceDb;
use ruff_db::files::Files;
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use ruff_db::vendored::{VendoredFileSystem, VendoredFileSystemBuilder};
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{SearchPathSettings, SearchPaths};
use ty_site_packages::{PythonEnvironment, SysPrefixPathOrigin};

static EMPTY_VENDORED: std::sync::LazyLock<VendoredFileSystem> = std::sync::LazyLock::new(|| {
    let mut builder = VendoredFileSystemBuilder::new(CompressionMethod::Stored);
    builder.add_file("stdlib/VERSIONS", "\n").unwrap();
    builder.finish().unwrap()
});

#[salsa::db]
#[derive(Clone)]
pub struct ModuleDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: OsSystem,
    search_paths: Arc<SearchPaths>,
    python_version: PythonVersion,
}

impl ModuleDb {
    /// Initialize a [`ModuleDb`] from the given source root.
    pub fn from_src_roots(
        src_roots: Vec<SystemPathBuf>,
        python_version: PythonVersion,
        venv_path: Option<SystemPathBuf>,
    ) -> Result<Self> {
        let system = OsSystem::default();
        let mut search_path_settings = SearchPathSettings::new(src_roots);
        // TODO: Consider calling `PythonEnvironment::discover` if the `venv_path` is not provided.
        if let Some(venv_path) = venv_path {
            let environment =
                PythonEnvironment::new(venv_path, SysPrefixPathOrigin::PythonCliFlag, &system)?;
            search_path_settings.site_packages_paths = environment
                .site_packages_paths(&system)
                .context("Failed to discover the site-packages directory")?
                .into_vec();
        }
        let search_paths = search_path_settings
            .to_search_paths(&system, &EMPTY_VENDORED)
            .context("Invalid search path settings")?;

        let db = Self {
            storage: salsa::Storage::new(None),
            files: Files::default(),
            system,
            search_paths: Arc::new(search_paths),
            python_version,
        };

        // Register the static roots for salsa durability
        db.search_paths.try_register_static_roots(&db);

        Ok(db)
    }
}

#[salsa::db]
impl SourceDb for ModuleDb {
    fn vendored(&self) -> &VendoredFileSystem {
        &EMPTY_VENDORED
    }

    fn system(&self) -> &dyn System {
        &self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }

    fn python_version(&self) -> PythonVersion {
        self.python_version
    }
}

#[salsa::db]
impl ty_module_resolver::Db for ModuleDb {
    fn search_paths(&self) -> &SearchPaths {
        &self.search_paths
    }
}

#[salsa::db]
impl salsa::Database for ModuleDb {}
