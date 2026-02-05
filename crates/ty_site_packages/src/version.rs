//! Types for representing the Python version and its source.

use std::sync::Arc;

use ruff_db::Db;
use ruff_db::diagnostic::Span;
use ruff_db::files::system_path_to_file;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::PythonVersion;
use ruff_text_size::TextRange;

/// The source of the Python version.
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

    /// Returns the path to the configuration file.
    pub fn path(&self) -> &SystemPathBuf {
        &self.path
    }

    /// Returns the range of the configuration setting.
    pub fn range(&self) -> Option<TextRange> {
        self.range
    }

    /// Attempt to resolve a [`Span`] that corresponds to the location of
    /// the configuration setting that specified the Python version.
    ///
    /// Useful for subdiagnostics when informing the user
    /// what the inferred Python version of their project is.
    pub fn span(&self, db: &dyn Db) -> Option<Span> {
        let file = system_path_to_file(db, &*self.path).ok()?;
        Some(Span::from(file).with_optional_range(self.range))
    }
}

/// A Python version with its source.
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
