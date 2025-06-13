use std::borrow::Cow;
use std::sync::Arc;

use crate::Db;
use crate::module_resolver::SearchPaths;
use crate::python_platform::PythonPlatform;
use crate::site_packages::SysPrefixPathOrigin;

use anyhow::Context;
use ruff_db::diagnostic::Span;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;
use ruff_text_size::TextRange;
use salsa::Durability;
use salsa::Setter;

#[salsa::input(singleton)]
pub struct Program {
    #[returns(ref)]
    pub python_version_with_source: PythonVersionWithSource,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(ref)]
    pub(crate) search_paths: SearchPaths,
}

impl Program {
    pub fn from_settings(db: &dyn Db, settings: ProgramSettings) -> anyhow::Result<Self> {
        let ProgramSettings {
            python_version: python_version_with_source,
            python_platform,
            search_paths,
        } = settings;

        let search_paths = SearchPaths::from_settings(db, &search_paths)
            .with_context(|| "Invalid search path settings")?;

        let python_version_with_source =
            Self::resolve_python_version(python_version_with_source, &search_paths);

        tracing::info!(
            "Python version: Python {python_version}, platform: {python_platform}",
            python_version = python_version_with_source.version
        );

        Ok(
            Program::builder(python_version_with_source, python_platform, search_paths)
                .durability(Durability::HIGH)
                .new(db),
        )
    }

    pub fn python_version(self, db: &dyn Db) -> PythonVersion {
        self.python_version_with_source(db).version
    }

    fn resolve_python_version(
        config_value: Option<PythonVersionWithSource>,
        search_paths: &SearchPaths,
    ) -> PythonVersionWithSource {
        config_value
            .or_else(|| {
                search_paths
                    .try_resolve_installation_python_version()
                    .map(Cow::into_owned)
            })
            .unwrap_or_default()
    }

    pub fn update_from_settings(
        self,
        db: &mut dyn Db,
        settings: ProgramSettings,
    ) -> anyhow::Result<()> {
        let ProgramSettings {
            python_version: python_version_with_source,
            python_platform,
            search_paths,
        } = settings;

        let search_paths = SearchPaths::from_settings(db, &search_paths)?;

        let new_python_version =
            Self::resolve_python_version(python_version_with_source, &search_paths);

        if self.search_paths(db) != &search_paths {
            tracing::debug!("Updating search paths");
            self.set_search_paths(db).to(search_paths);
        }

        if &python_platform != self.python_platform(db) {
            tracing::debug!("Updating python platform: `{python_platform:?}`");
            self.set_python_platform(db).to(python_platform);
        }

        if &new_python_version != self.python_version_with_source(db) {
            tracing::debug!(
                "Updating python version: Python {version}",
                version = new_python_version.version
            );
            self.set_python_version_with_source(db)
                .to(new_python_version);
        }

        Ok(())
    }

    /// Update the search paths for the program.
    pub fn update_search_paths(
        self,
        db: &mut dyn Db,
        search_path_settings: &SearchPathSettings,
    ) -> anyhow::Result<()> {
        let search_paths = SearchPaths::from_settings(db, search_path_settings)?;

        let current_python_version = self.python_version_with_source(db);

        let python_version_from_environment = search_paths
            .try_resolve_installation_python_version()
            .map(Cow::into_owned)
            .unwrap_or_default();

        if current_python_version != &python_version_from_environment
            && current_python_version.source.priority()
                <= python_version_from_environment.source.priority()
        {
            tracing::debug!("Updating Python version from environment");
            self.set_python_version_with_source(db)
                .to(python_version_from_environment);
        }

        if self.search_paths(db) != &search_paths {
            tracing::debug!("Updating search paths");
            self.set_search_paths(db).to(search_paths);
        }

        Ok(())
    }

    pub fn custom_stdlib_search_path(self, db: &dyn Db) -> Option<&SystemPath> {
        self.search_paths(db).custom_stdlib()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramSettings {
    pub python_version: Option<PythonVersionWithSource>,
    pub python_platform: PythonPlatform,
    pub search_paths: SearchPathSettings,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
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

    /// We fell back to a default value because the value was not specified via the CLI or a config file.
    #[default]
    Default,
}

impl PythonVersionSource {
    fn priority(&self) -> PythonSourcePriority {
        match self {
            PythonVersionSource::Default => PythonSourcePriority::Default,
            PythonVersionSource::PyvenvCfgFile(_) => PythonSourcePriority::PyvenvCfgFile,
            PythonVersionSource::ConfigFile(_) => PythonSourcePriority::ConfigFile,
            PythonVersionSource::Cli => PythonSourcePriority::Cli,
            PythonVersionSource::InstallationDirectoryLayout { .. } => {
                PythonSourcePriority::InstallationDirectoryLayout
            }
        }
    }
}

/// The priority in which Python version sources are considered.
/// The lower down the variant appears in this enum, the higher its priority.
///
/// For example, if a Python version is specified in a pyproject.toml file
/// but *also* via a CLI argument, the CLI argument will take precedence.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
enum PythonSourcePriority {
    Default,
    InstallationDirectoryLayout,
    PyvenvCfgFile,
    ConfigFile,
    Cli,
}

/// Information regarding the file and [`TextRange`] of the configuration
/// from which we inferred the Python version.
#[derive(Debug, PartialEq, Eq, Clone)]
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
        let file = system_path_to_file(db.upcast(), &*self.path).ok()?;
        Some(Span::from(file).with_optional_range(self.range))
    }
}

#[derive(Eq, PartialEq, Debug, Clone)]
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

    /// Path to the Python installation from which ty resolves third party dependencies
    /// and their type information.
    pub python_path: PythonPath,
}

impl SearchPathSettings {
    pub fn new(src_roots: Vec<SystemPathBuf>) -> Self {
        Self {
            src_roots,
            extra_paths: vec![],
            custom_typeshed: None,
            python_path: PythonPath::KnownSitePackages(vec![]),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PythonPath {
    /// A path that either represents the value of [`sys.prefix`] at runtime in Python
    /// for a given Python executable, or which represents a path relative to `sys.prefix`
    /// that we will attempt later to resolve into `sys.prefix`. Exactly which this variant
    /// represents depends on the [`SysPrefixPathOrigin`] element in the tuple.
    ///
    /// For the case of a virtual environment, where a
    /// Python binary is at `/.venv/bin/python`, `sys.prefix` is the path to
    /// the virtual environment the Python binary lies inside, i.e. `/.venv`,
    /// and `site-packages` will be at `.venv/lib/python3.X/site-packages`.
    /// System Python installations generally work the same way: if a system
    /// Python installation lies at `/opt/homebrew/bin/python`, `sys.prefix`
    /// will be `/opt/homebrew`, and `site-packages` will be at
    /// `/opt/homebrew/lib/python3.X/site-packages`.
    ///
    /// [`sys.prefix`]: https://docs.python.org/3/library/sys.html#sys.prefix
    IntoSysPrefix(SystemPathBuf, SysPrefixPathOrigin),

    /// Resolved site packages paths.
    ///
    /// This variant is mainly intended for testing where we want to skip resolving `site-packages`
    /// because it would unnecessarily complicate the test setup.
    KnownSitePackages(Vec<SystemPathBuf>),
}

impl PythonPath {
    pub fn sys_prefix(path: impl Into<SystemPathBuf>, origin: SysPrefixPathOrigin) -> Self {
        Self::IntoSysPrefix(path.into(), origin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn test_python_version_source_priority() {
        for priority in PythonSourcePriority::iter() {
            match priority {
                // CLI source takes priority over all other sources.
                PythonSourcePriority::Cli => {
                    for other in PythonSourcePriority::iter() {
                        assert!(priority >= other, "{other:?}");
                    }
                }
                // Config files have lower priority than CLI arguments,
                // but higher than pyvenv.cfg files and the fallback default.
                PythonSourcePriority::ConfigFile => {
                    for other in PythonSourcePriority::iter() {
                        match other {
                            PythonSourcePriority::Cli => assert!(other > priority, "{other:?}"),
                            PythonSourcePriority::ConfigFile => assert_eq!(priority, other),
                            PythonSourcePriority::PyvenvCfgFile
                            | PythonSourcePriority::Default
                            | PythonSourcePriority::InstallationDirectoryLayout => {
                                assert!(priority > other, "{other:?}");
                            }
                        }
                    }
                }
                // Pyvenv.cfg files have lower priority than CLI flags and config files,
                // but higher than the default fallback.
                PythonSourcePriority::PyvenvCfgFile => {
                    for other in PythonSourcePriority::iter() {
                        match other {
                            PythonSourcePriority::Cli | PythonSourcePriority::ConfigFile => {
                                assert!(other > priority, "{other:?}");
                            }
                            PythonSourcePriority::PyvenvCfgFile => assert_eq!(priority, other),
                            PythonSourcePriority::Default
                            | PythonSourcePriority::InstallationDirectoryLayout => {
                                assert!(priority > other, "{other:?}");
                            }
                        }
                    }
                }
                PythonSourcePriority::InstallationDirectoryLayout => {
                    for other in PythonSourcePriority::iter() {
                        match other {
                            PythonSourcePriority::Cli
                            | PythonSourcePriority::ConfigFile
                            | PythonSourcePriority::PyvenvCfgFile => {
                                assert!(other > priority, "{other:?}");
                            }
                            PythonSourcePriority::InstallationDirectoryLayout => {
                                assert_eq!(priority, other);
                            }
                            PythonSourcePriority::Default => assert!(priority > other, "{other:?}"),
                        }
                    }
                }
                PythonSourcePriority::Default => {
                    for other in PythonSourcePriority::iter() {
                        assert!(priority <= other, "{other:?}");
                    }
                }
            }
        }
    }
}
