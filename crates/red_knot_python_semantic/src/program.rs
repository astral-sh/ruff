use crate::module_resolver::SearchPaths;
use crate::python_platform::PythonPlatform;
use crate::python_version::PythonVersion;
use crate::Db;

use anyhow::Context;
use ruff_db::system::{SystemPath, SystemPathBuf};
use salsa::Durability;
use salsa::Setter;

#[salsa::input(singleton)]
pub struct Program {
    pub python_version: PythonVersion,

    #[return_ref]
    pub python_platform: PythonPlatform,

    #[return_ref]
    pub(crate) search_paths: SearchPaths,
}

impl Program {
    pub fn from_settings(db: &dyn Db, settings: ProgramSettings) -> anyhow::Result<Self> {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        tracing::info!("Python version: Python {python_version}, platform: {python_platform}");

        let search_paths = SearchPaths::from_settings(db, &search_paths)
            .with_context(|| "Invalid search path settings")?;

        Ok(
            Program::builder(python_version, python_platform, search_paths)
                .durability(Durability::HIGH)
                .new(db),
        )
    }

    pub fn update_from_settings(
        self,
        db: &mut dyn Db,
        settings: ProgramSettings,
    ) -> anyhow::Result<()> {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        if &python_platform != self.python_platform(db) {
            tracing::debug!("Updating python platform: `{python_platform:?}`");
            self.set_python_platform(db).to(python_platform);
        }

        if python_version != self.python_version(db) {
            tracing::debug!("Updating python version: Python {python_version}");
            self.set_python_version(db).to(python_version);
        }

        self.update_search_paths(db, &search_paths)?;

        Ok(())
    }

    pub fn update_search_paths(
        self,
        db: &mut dyn Db,
        search_path_settings: &SearchPathSettings,
    ) -> anyhow::Result<()> {
        let search_paths = SearchPaths::from_settings(db, search_path_settings)?;

        if self.search_paths(db) != &search_paths {
            tracing::debug!("Update search paths");
            self.set_search_paths(db).to(search_paths);
        }

        Ok(())
    }

    pub fn custom_stdlib_search_path(self, db: &dyn Db) -> Option<&SystemPath> {
        self.search_paths(db).custom_stdlib()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ProgramSettings {
    pub python_version: PythonVersion,
    pub python_platform: PythonPlatform,
    pub search_paths: SearchPathSettings,
}

/// Configures the search paths for module resolution.
#[derive(Eq, PartialEq, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SearchPathSettings {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Vec<SystemPathBuf>,

    /// The root of the project, used for finding first-party modules.
    pub src_roots: Vec<SystemPathBuf>,

    /// Optional path to a "custom typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    pub custom_typeshed: Option<SystemPathBuf>,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: SitePackages,
}

impl SearchPathSettings {
    pub fn new(src_roots: Vec<SystemPathBuf>) -> Self {
        Self {
            src_roots,
            extra_paths: vec![],
            custom_typeshed: None,
            site_packages: SitePackages::Known(vec![]),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SitePackages {
    Derived {
        venv_path: SystemPathBuf,
    },
    /// Resolved site packages paths
    Known(Vec<SystemPathBuf>),
}
