use anyhow::{Context, Result};
use std::collections::HashSet;
use std::panic::RefUnwindSafe;
use std::sync::Arc;
use zip::CompressionMethod;

use ruff_db::Db as SourceDb;
use ruff_db::files::Files;
use ruff_db::system::{System, SystemPathBuf};
use ruff_db::vendored::{VendoredFileSystem, VendoredFileSystemBuilder};
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{FallibleStrategy, SearchPath, SearchPathSettings, SearchPaths};
use ty_site_packages::{PythonEnvironment, SysPrefixPathOrigin};

static EMPTY_VENDORED: std::sync::LazyLock<VendoredFileSystem> = std::sync::LazyLock::new(|| {
    let mut builder = VendoredFileSystemBuilder::new(CompressionMethod::Stored);
    builder.add_file("stdlib/VERSIONS", "\n").unwrap();
    builder.finish().unwrap()
});

#[salsa::db]
#[derive(Clone)]
pub struct ImportDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: Arc<dyn System + Send + Sync + RefUnwindSafe>,
    search_paths: Arc<SearchPaths>,
    python_version: PythonVersion,
    root_paths: Arc<[SystemPathBuf]>,
}

impl ImportDb {
    /// Initialize an [`ImportDb`] from source roots and, optionally, a Python environment root.
    pub fn from_src_roots<S>(
        system: S,
        src_roots: Vec<SystemPathBuf>,
        python_version: PythonVersion,
        venv_path: Option<SystemPathBuf>,
    ) -> Result<Self>
    where
        S: System + 'static + Send + Sync + RefUnwindSafe,
    {
        let site_packages_paths = if let Some(venv_path) = venv_path {
            let environment =
                PythonEnvironment::new(venv_path, SysPrefixPathOrigin::PythonCliFlag, &system)?;
            environment
                .site_packages_paths(&system)
                .context("Failed to discover the site-packages directory")?
                .into_vec()
        } else {
            Vec::new()
        };

        Self::from_roots(system, src_roots, site_packages_paths, python_version)
    }

    /// Initialize an [`ImportDb`] from explicit first-party and site-packages roots.
    pub fn from_roots<S>(
        system: S,
        src_roots: Vec<SystemPathBuf>,
        site_packages_paths: Vec<SystemPathBuf>,
        python_version: PythonVersion,
    ) -> Result<Self>
    where
        S: System + 'static + Send + Sync + RefUnwindSafe,
    {
        let mut search_path_settings = SearchPathSettings::new(src_roots.clone());
        search_path_settings.site_packages_paths = site_packages_paths.clone();

        let search_paths = search_path_settings
            .to_search_paths(&system, &EMPTY_VENDORED, &FallibleStrategy)
            .context("Invalid search path settings")?;

        let root_paths = canonical_root_paths(&system, src_roots, site_packages_paths);

        let db = Self {
            storage: salsa::Storage::new(None),
            files: Files::default(),
            system: Arc::new(system),
            search_paths: Arc::new(search_paths),
            python_version,
            root_paths: Arc::from(root_paths),
        };

        // Register the static roots for salsa durability.
        db.search_paths.try_register_static_roots(&db);

        Ok(db)
    }

    pub fn winning_root_index(&self, search_path: &SearchPath) -> Option<usize> {
        let path = search_path.as_system_path()?;
        self.root_paths
            .iter()
            .position(|root| root.as_path() == path)
    }
}

fn canonical_root_paths(
    system: &dyn System,
    src_roots: Vec<SystemPathBuf>,
    site_packages_paths: Vec<SystemPathBuf>,
) -> Vec<SystemPathBuf> {
    let mut seen = HashSet::new();
    let mut root_paths = Vec::new();

    for path in src_roots.into_iter().chain(site_packages_paths) {
        let path = system
            .canonicalize_path(&path)
            .unwrap_or_else(|_| path.to_path_buf());
        if seen.insert(path.clone()) {
            root_paths.push(path);
        }
    }

    root_paths
}

#[salsa::db]
impl SourceDb for ImportDb {
    fn vendored(&self) -> &VendoredFileSystem {
        &EMPTY_VENDORED
    }

    fn system(&self) -> &dyn System {
        &*self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }

    fn python_version(&self) -> PythonVersion {
        self.python_version
    }
}

#[salsa::db]
impl ty_module_resolver::Db for ImportDb {
    fn search_paths(&self) -> &SearchPaths {
        &self.search_paths
    }
}

#[salsa::db]
impl salsa::Database for ImportDb {}
