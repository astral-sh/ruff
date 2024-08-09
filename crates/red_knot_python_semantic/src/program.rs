use crate::python_version::PythonVersion;
use crate::Db;
use ruff_db::system::SystemPathBuf;
use salsa::Durability;

#[salsa::input(singleton)]
pub struct Program {
    pub target_version: PythonVersion,

    #[return_ref]
    pub search_paths: SearchPathSettings,
}

impl Program {
    pub fn from_settings(db: &dyn Db, settings: ProgramSettings) -> Self {
        Program::builder(settings.target_version, settings.search_paths)
            .durability(Durability::HIGH)
            .new(db)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ProgramSettings {
    pub target_version: PythonVersion,
    pub search_paths: SearchPathSettings,
}

/// Configures the search paths for module resolution.
#[derive(Eq, PartialEq, Debug, Clone, Default)]
pub struct SearchPathSettings {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Vec<SystemPathBuf>,

    /// The root of the workspace, used for finding first-party modules.
    pub src_root: SystemPathBuf,

    /// Optional path to a "custom typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    pub custom_typeshed: Option<SystemPathBuf>,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: Vec<SystemPathBuf>,
}
