use std::sync::Arc;

use crate::Db;
use crate::module_resolver::{SearchPathValidationError, SearchPaths};
use crate::python_platform::PythonPlatform;

use ruff_db::diagnostic::Span;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use ruff_text_size::TextRange;
use salsa::Durability;
use salsa::Setter;

#[salsa::input(singleton, heap_size=ruff_memory_usage::heap_size)]
pub struct Program {
    #[returns(ref)]
    pub python_version_with_source: PythonVersionWithSource,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(ref)]
    pub(crate) search_paths: SearchPaths,
}

impl Program {
    pub fn init_or_update(db: &mut dyn Db, settings: ProgramSettings) -> Self {
        match Self::try_get(db) {
            Some(program) => {
                program.update_from_settings(db, settings);
                program
            }
            None => Self::from_settings(db, settings),
        }
    }

    pub fn from_settings(db: &dyn Db, settings: ProgramSettings) -> Self {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        search_paths.try_register_static_roots(db);

        Program::builder(python_version, python_platform, search_paths)
            .durability(Durability::HIGH)
            .new(db)
    }

    pub fn python_version(self, db: &dyn Db) -> PythonVersion {
        self.python_version_with_source(db).version
    }

    pub fn update_from_settings(self, db: &mut dyn Db, settings: ProgramSettings) {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        if self.search_paths(db) != &search_paths {
            tracing::debug!("Updating search paths");
            search_paths.try_register_static_roots(db);
            self.set_search_paths(db).to(search_paths);
        }

        if &python_platform != self.python_platform(db) {
            tracing::debug!("Updating python platform: `{python_platform:?}`");
            self.set_python_platform(db).to(python_platform);
        }

        if &python_version != self.python_version_with_source(db) {
            tracing::debug!(
                "Updating python version: Python {version}",
                version = python_version.version
            );
            self.set_python_version_with_source(db).to(python_version);
        }
    }

    pub fn custom_stdlib_search_path(self, db: &dyn Db) -> Option<&SystemPath> {
        self.search_paths(db).custom_stdlib()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramSettings {
    pub python_version: PythonVersionWithSource,
    pub python_platform: PythonPlatform,
    pub search_paths: SearchPaths,
}

#[derive(Clone, Debug, Eq, PartialEq, Default, get_size2::GetSize)]
pub enum PythonVersionSource {
    /// Value loaded from a project's configuration file.
    ConfigFile(PythonVersionFileSource),

    /// Value loaded from the `pyvenv.cfg` file of the virtual environment.
    /// The virtual environment might have been configured, activated or inferred.
    PyvenvCfgFile(PythonVersionFileSource),

    /// Value inferred from the layout of the Python installation.
    ///
    /// This only ever applies on Unix. On Unix, the `site-packages` directory
    /// will always be at `sys.prefix/lib/pythonX.Y/site-packages`,
    /// so we can infer the Python version from the parent directory of `site-packages`.
    InstallationDirectoryLayout { site_packages_parent_dir: Box<str> },

    /// The value comes from a CLI argument, while it's left open if specified using a short argument,
    /// long argument (`--extra-paths`) or `--config key=value`.
    Cli,

    /// The value comes from the user's editor,
    /// while it's left open if specified as a setting
    /// or if the value was auto-discovered by the editor
    /// (e.g., the Python environment)
    Editor,

    /// We fell back to a default value because the value was not specified via the CLI or a config file.
    #[default]
    Default,
}

/// Information regarding the file and [`TextRange`] of the configuration
/// from which we inferred the Python version.
#[derive(Debug, PartialEq, Eq, Clone, get_size2::GetSize)]
pub struct PythonVersionFileSource {
    path: Arc<SystemPathBuf>,
    range: Option<TextRange>,
}

impl PythonVersionFileSource {
    pub fn new(path: Arc<SystemPathBuf>, range: Option<TextRange>) -> Self {
        Self { path, range }
    }

    /// Attempt to resolve a [`Span`] that corresponds to the location of
    /// the configuration setting that specified the Python version.
    ///
    /// Useful for subdiagnostics when informing the user
    /// what the inferred Python version of their project is.
    pub(crate) fn span(&self, db: &dyn Db) -> Option<Span> {
        let file = system_path_to_file(db, &*self.path).ok()?;
        Some(Span::from(file).with_optional_range(self.range))
    }
}

#[derive(Eq, PartialEq, Debug, Clone, get_size2::GetSize)]
pub struct PythonVersionWithSource {
    pub version: PythonVersion,
    pub source: PythonVersionSource,
}

impl Default for PythonVersionWithSource {
    fn default() -> Self {
        Self {
            version: PythonVersion::latest_ty(),
            source: PythonVersionSource::Default,
        }
    }
}

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
        }
    }

    pub fn to_search_paths(
        &self,
        system: &dyn System,
        vendored: &VendoredFileSystem,
    ) -> Result<SearchPaths, SearchPathValidationError> {
        SearchPaths::from_settings(self, system, vendored)
    }
}
