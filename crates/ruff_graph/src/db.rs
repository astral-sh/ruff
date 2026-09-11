use anyhow::{Context, Result};
use std::panic::RefUnwindSafe;
use std::sync::Arc;
use zip::CompressionMethod;

use ruff_db::Db as SourceDb;
use ruff_db::files::Files;
use ruff_db::system::{System, SystemPathBuf};
use ruff_db::vendored::{VendoredFileSystem, VendoredFileSystemBuilder};
use ty_module_resolver::{FallibleStrategy, SearchPathSettings, SearchPaths};
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
    system: Arc<dyn System + Send + Sync + RefUnwindSafe>,
}

impl ModuleDb {
    /// Initialize a [`ModuleDb`] for the given system.
    pub fn new<S>(system: S) -> Self
    where
        S: System + 'static + Send + Sync + RefUnwindSafe,
    {
        Self {
            storage: salsa::Storage::new(None),
            files: Files::default(),
            system: Arc::new(system),
        }
    }
}

/// Resolve module search paths for the given source roots and Python environment.
pub fn resolve_search_paths(
    system: &dyn System,
    src_roots: Vec<SystemPathBuf>,
    venv_path: Option<SystemPathBuf>,
) -> Result<SearchPaths> {
    let mut search_path_settings = SearchPathSettings::new(src_roots);
    // TODO: Consider calling `PythonEnvironment::discover` if the `venv_path` is not provided.
    if let Some(venv_path) = venv_path {
        let environment =
            PythonEnvironment::new(venv_path, SysPrefixPathOrigin::PythonCliFlag, system)?;
        search_path_settings.site_packages_paths = environment
            .site_packages_paths(system)
            .context("Failed to discover the site-packages directory")?
            .into_vec();
    }

    search_path_settings
        .to_search_paths(system, &EMPTY_VENDORED, &FallibleStrategy)
        .context("Invalid search path settings")
}

#[salsa::db]
impl SourceDb for ModuleDb {
    fn vendored(&self) -> &VendoredFileSystem {
        &EMPTY_VENDORED
    }

    fn system(&self) -> &dyn System {
        &*self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }
}

#[salsa::db]
impl ty_module_resolver::Db for ModuleDb {}

#[salsa::db]
impl salsa::Database for ModuleDb {}
