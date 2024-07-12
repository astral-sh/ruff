use crate::system::SystemPathBuf;

/// A program that is analyzed as a whole.
///
/// ## How is it different from [`Workspace](`crate::workspace::Workspace`)?
/// Workspace is a representation of the project structure and its configuration.
/// [`Program`] is a narrower view of [`Workspace`] that focuses on the state relevant for type checking,
/// but ignores e.g. formatter or linter settings.
#[salsa::input(singleton)]
pub struct Program {
    /// The target version that this program checks for
    pub target_version: TargetVersion,

    #[return_ref]
    pub module_resolution_settings: RawModuleResolutionSettings,
}

/// Enumeration of all supported Python versions
///
/// TODO: unify with the `PythonVersion` enum in the linter/formatter crates?
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum TargetVersion {
    Py37,
    #[default]
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

/// "Raw" configuration settings for module resolution: unvalidated, unnormalized
#[derive(Eq, PartialEq, Debug)]
pub struct RawModuleResolutionSettings {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Vec<SystemPathBuf>,

    /// The root of the program, used for finding first-party modules.
    pub workspace_root: SystemPathBuf,

    /// Optional (already validated) path to standard-library typeshed stubs.
    /// If this is not provided, we will fallback to our vendored typeshed stubs
    /// bundled as a zip file in the binary
    pub custom_typeshed: Option<SystemPathBuf>,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: Option<SystemPathBuf>,
}
