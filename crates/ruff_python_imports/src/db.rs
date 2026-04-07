use std::panic::RefUnwindSafe;
use std::sync::Arc;

use anyhow::{Context, Result};
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
    /// Canonical root paths in registration order (first-party roots, then site-packages).
    /// Used to map a [`SearchPath`] back to a stable index for downstream ownership classification.
    root_paths: Arc<[SystemPathBuf]>,
}

impl ImportDb {
    /// Initialize an [`ImportDb`] from the given source roots and an optional virtual environment.
    ///
    /// Site-packages paths are discovered from `venv_path` when provided.
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
        search_path_settings
            .site_packages_paths
            .clone_from(&site_packages_paths);

        let search_paths = search_path_settings
            .to_search_paths(&system, &EMPTY_VENDORED, &FallibleStrategy)
            .context("Invalid search path settings")?;

        let root_paths = deduplicated_root_paths(&system, src_roots, site_packages_paths);

        let db = Self {
            storage: salsa::Storage::new(None),
            files: Files::default(),
            system: Arc::new(system),
            search_paths: Arc::new(search_paths),
            python_version,
            root_paths: Arc::from(root_paths),
        };

        db.search_paths.try_register_static_roots(&db);

        Ok(db)
    }

    /// Map a [`SearchPath`] back to its index in the configured root ordering.
    ///
    /// The index corresponds to the concatenation of `src_roots` then `site_packages_paths`
    /// as passed to the constructor, after canonicalization and deduplication.
    ///
    /// Returns `None` if the search path does not correspond to one of those configured roots.
    pub(crate) fn winning_root_index(&self, search_path: &SearchPath) -> Option<usize> {
        let path = search_path.as_system_path()?;
        let path = self
            .system
            .canonicalize_path(path)
            .unwrap_or_else(|_| path.to_path_buf());
        self.root_paths.iter().position(|root| root == &path)
    }
}

/// Canonicalize and deduplicate root paths, preserving insertion order.
pub(crate) fn deduplicated_root_paths(
    system: &dyn System,
    src_roots: Vec<SystemPathBuf>,
    site_packages_paths: Vec<SystemPathBuf>,
) -> Vec<SystemPathBuf> {
    let mut root_paths = Vec::new();

    for path in src_roots.into_iter().chain(site_packages_paths) {
        let path = system.canonicalize_path(&path).unwrap_or(path);
        if !root_paths.contains(&path) {
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
