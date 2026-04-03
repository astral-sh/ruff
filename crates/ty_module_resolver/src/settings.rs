//! Search path configuration settings.

use ruff_db::system::{System, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;

use crate::path::SearchPathError;
use crate::resolve::SearchPaths;
use crate::strategy::MisconfigurationStrategy;
use crate::typeshed::TypeshedVersionsParseError;

/// Configures the search paths for module resolution.
#[derive(Eq, PartialEq, Debug, Clone)]
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

    /// List of site packages paths to use.
    pub site_packages_paths: Vec<SystemPathBuf>,

    /// Option path to the real stdlib on the system, and not some instance of typeshed.
    ///
    /// We should ideally only ever use this for things like goto-definition,
    /// where typeshed isn't the right answer.
    pub real_stdlib_path: Option<SystemPathBuf>,

    /// Whether the user explicitly configured first-party source roots
    /// (via `src.root` or `environment.root`).
    /// When `true`, desperate resolution skips src-layout heuristics for
    /// discovered sub-projects; when `false`, it applies them.
    pub has_explicit_roots: bool,
}

impl SearchPathSettings {
    pub fn new(src_roots: Vec<SystemPathBuf>) -> Self {
        Self {
            src_roots,
            ..SearchPathSettings::empty()
        }
    }

    pub fn empty() -> Self {
        SearchPathSettings {
            src_roots: vec![],
            extra_paths: vec![],
            custom_typeshed: None,
            site_packages_paths: vec![],
            real_stdlib_path: None,
            has_explicit_roots: false,
        }
    }

    pub fn to_search_paths<Strategy: MisconfigurationStrategy>(
        &self,
        system: &dyn System,
        vendored: &VendoredFileSystem,
        strategy: &Strategy,
    ) -> Result<SearchPaths, Strategy::Error<SearchPathSettingsError>> {
        SearchPaths::from_settings(self, system, vendored, strategy)
    }
}

/// Enumeration describing the various ways in which validation of the search paths options might fail.
///
/// If validation fails for a search path derived from the user settings,
/// a message must be displayed to the user,
/// as type checking cannot be done reliably in these circumstances.
#[derive(Debug, thiserror::Error)]
pub enum SearchPathSettingsError {
    #[error(transparent)]
    InvalidSearchPath(#[from] SearchPathError),

    /// The typeshed path provided by the user is a directory,
    /// but `stdlib/VERSIONS` could not be read.
    /// (This is only relevant for stdlib search paths.)
    #[error("Failed to read the custom typeshed versions file '{path}'")]
    FailedToReadVersionsFile {
        path: SystemPathBuf,
        #[source]
        error: std::io::Error,
    },

    /// The path provided by the user is a directory,
    /// and a `stdlib/VERSIONS` file exists, but it fails to parse.
    /// (This is only relevant for stdlib search paths.)
    #[error(transparent)]
    VersionsParseError(#[from] TypeshedVersionsParseError),
}
