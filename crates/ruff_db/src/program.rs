use std::sync::Arc;

use crate::system::{SystemPath, SystemPathBuf};
use crate::vendored::VendoredPathBuf;

#[salsa::input(singleton)]
pub struct Program {
    pub target_version: TargetVersion,

    #[return_ref]
    pub search_path_settings: SearchPathSettings,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RawProgramSettings {
    pub target_version: TargetVersion,
    pub search_paths: RawSearchPathSettings,
}

/// Enumeration of all supported Python versions
///
/// TODO: unify with the `PythonVersion` enum in the linter/formatter crates?
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
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

impl TargetVersion {
    pub const fn as_tuple(self) -> (u8, u8) {
        match self {
            Self::Py37 => (3, 7),
            Self::Py38 => (3, 8),
            Self::Py39 => (3, 9),
            Self::Py310 => (3, 10),
            Self::Py311 => (3, 11),
            Self::Py312 => (3, 12),
            Self::Py313 => (3, 13),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Py37 => "py37",
            Self::Py38 => "py38",
            Self::Py39 => "py39",
            Self::Py310 => "py310",
            Self::Py311 => "py311",
            Self::Py312 => "py312",
            Self::Py313 => "py313",
        }
    }
}

impl std::fmt::Display for TargetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Debug for TargetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

/// Validated and normalized module-resolution settings.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct SearchPathSettings {
    /// Search paths that have been statically determined purely from reading Ruff's configuration settings.
    /// These shouldn't ever change unless the config settings themselves change.
    pub static_search_paths: Vec<SearchPath>,

    /// site-packages paths are not included in the above field:
    /// if there are multiple site-packages paths, editable installations can appear
    /// *between* the site-packages paths on `sys.path` at runtime.
    /// That means we can't know where a second or third `site-packages` path should sit
    /// in terms of module-resolution priority until we've discovered the editable installs
    /// for the first `site-packages` path
    pub site_packages_paths: Vec<SystemPathBuf>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum SearchPathInner {
    Extra(SystemPathBuf),
    FirstParty(SystemPathBuf),
    StandardLibraryCustom(SystemPathBuf),
    StandardLibraryVendored(VendoredPathBuf),
    SitePackages(SystemPathBuf),
    Editable(SystemPathBuf),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SearchPath(pub Arc<SearchPathInner>);

impl SearchPath {
    pub fn as_system_path(&self) -> Option<&SystemPath> {
        match &*self.0 {
            SearchPathInner::Extra(path)
            | SearchPathInner::FirstParty(path)
            | SearchPathInner::StandardLibraryCustom(path)
            | SearchPathInner::SitePackages(path)
            | SearchPathInner::Editable(path) => Some(path),
            SearchPathInner::StandardLibraryVendored(_) => None,
        }
    }
}

/// Unvalidated settings from the user that configure the search paths for module resolution.
#[derive(Eq, PartialEq, Debug, Clone, Default)]
pub struct RawSearchPathSettings {
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

    /// The path to the user's `site-packages` directories,
    /// where third-party packages from ``PyPI`` are installed.
    ///
    /// Usually there will either be 0 or 1 site-packages paths,
    /// but some environments are able to access both a venv's `site-packages`
    /// and the system installation's `site-packages`.
    pub site_packages: Vec<SystemPathBuf>,
}
