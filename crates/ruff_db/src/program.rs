// TODO: Fix clippy warnings in Salsa macros
#![allow(clippy::needless_lifetimes, clippy::clone_on_copy)]

use crate::{system::SystemPathBuf, Db};

#[salsa::input(singleton)]
pub struct Program {
    pub target_version: TargetVersion,

    #[return_ref]
    pub search_paths: SearchPaths,
}

impl Program {
    pub fn from_settings(db: &dyn Db, settings: ProgramSettings) -> Self {
        Program::new(db, settings.target_version, settings.search_paths)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ProgramSettings {
    pub target_version: TargetVersion,
    pub search_paths: SearchPaths,
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

/// "Raw" configuration settings for module resolution: unvalidated, unnormalized
#[derive(Eq, PartialEq, Debug)]
pub struct SearchPaths {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Vec<SystemPathBuf>,

    /// The root of the workspace, used for finding first-party modules.
    pub workspace_root: SystemPathBuf,

    /// Optional (already validated) path to standard-library typeshed stubs.
    /// If this is not provided, we will fallback to our vendored typeshed stubs
    /// bundled as a zip file in the binary
    pub custom_typeshed: Option<SystemPathBuf>,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: Option<SystemPathBuf>,
}
