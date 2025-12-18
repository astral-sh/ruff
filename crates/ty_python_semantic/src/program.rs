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

/// A type that never exists.
///
/// In Rust if you have Result<T, Never> the compiler knows `Err` is impossible
/// and you can just write `let Ok(val) = result;`
pub enum Never {}

/// Generic handling of two possible approaches to an Error:
///
/// * [`FailStrategy`]: The code should simply fail
/// * [`UseDefaultStrategy`]: The chode should apply default values and never fail
///
/// Any function that wants to be made generic over these approaches should be changed thusly.
///
/// Old:
///
/// ```ignore
/// fn do_thing()
///     -> Result<T, E>
/// {
///     let x = something_fallible()?;
///     Ok(x)
/// }
/// ```
///
/// New:
///
/// ```ignore
/// fn do_thing<Strategy: MisconfigurationStrategy>(strategy: &Strategy)
///     -> Result<T, Strategy::Error<E>>
/// {
///     let x = strategy.fallback(something_fallible(), |err| {
///         tracing::debug!("Failed to get value: {err}");
///         MyType::default()
///     })?;
///     Ok(x)
/// }
/// ```
///
/// The key trick is instead of returning `Result<T, E>` your function should
/// return `Result<T, Strategy::Error<E>`. Which simplifies to:
///
/// * [`FailStrategy`]: `Result<T, E>`
/// * [`UseDefaultStrategy`]: `Result<T, Never>` ~= `T`
///
/// Notably, if your function returns `Result<T, Strategy::Error<E>>` you will
/// be *statically prevented* from returning an `Err` without going through
/// [`MisconfigurationStrategy::fallback`][] or [`MisconfigurationStrategy::fallback_opt`][]
/// which ensure you're handling both approaches (or you wrote an `unwrap` but
/// those standout far more than adding a new `?` to a function that must be able to Not Fail.
///
/// Also, for any caller that passes in [`UseDefaultStrategy`], they will be able
/// to write `let Ok(val) = do_thing(&UseDefaultStrategy);` instead of having to
/// write an `unwrap()`.
pub trait MisconfigurationStrategy {
    /// * [`FailStrategy`]: `E`
    /// * [`UseDefaultStrategy`]: `Never`
    type Error<E>;

    /// Try to get the value out of a Result that we need to proceed.
    ///
    /// If [`UseDefaultStrategy`], on `Err` this will call `fallback_fn` to compute
    /// a default value and always return `Ok`.
    ///
    /// If [`FailStrategy`] this is a no-op and will return the Result.
    fn fallback<T, E>(
        &self,
        result: Result<T, E>,
        fallback_fn: impl FnOnce(E) -> T,
    ) -> Result<T, Self::Error<E>>;

    /// Try to get the value out of a Result that we can do without.
    ///
    /// If [`UseDefaultStrategy`], this will call `fallback_fn` to report an issue
    /// (i.e. you can invoke `tracing::debug!` or something) and then return `None`.
    ///
    /// If [`FailStrategy`] this is a no-op and will return the Result (but `Ok` => `Ok(Some)`).
    fn fallback_opt<T, E>(
        &self,
        result: Result<T, E>,
        fallback_fn: impl FnOnce(E),
    ) -> Result<Option<T>, Self::Error<E>>;

    /// Convenience to convert the inner `Error` to `anyhow::Error`
    fn to_anyhow<T, E>(
        &self,
        result: Result<T, Self::Error<E>>,
    ) -> Result<T, Self::Error<anyhow::Error>>
    where
        anyhow::Error: From<E>;

    /// Convenience to map the inner `Error`
    fn map_err<T, E1, E2>(
        &self,
        result: Result<T, Self::Error<E1>>,
        map_err: impl FnOnce(E1) -> E2,
    ) -> Result<T, Self::Error<E2>>;
}

/// A [`MisconfigurationStrategy`] that refuses to *ever* return an `Err`
/// and instead substitutes default values or skips functionality.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct UseDefaultStrategy;

impl MisconfigurationStrategy for UseDefaultStrategy {
    type Error<E> = Never;
    fn fallback<T, E>(
        &self,
        result: Result<T, E>,
        fallback_fn: impl FnOnce(E) -> T,
    ) -> Result<T, Self::Error<E>> {
        Ok(result.unwrap_or_else(fallback_fn))
    }

    fn fallback_opt<T, E>(
        &self,
        result: Result<T, E>,
        fallback_fn: impl FnOnce(E),
    ) -> Result<Option<T>, Self::Error<E>> {
        match result {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                fallback_fn(e);
                Ok(None)
            }
        }
    }

    fn to_anyhow<T, E>(
        &self,
        result: Result<T, Self::Error<E>>,
    ) -> Result<T, Self::Error<anyhow::Error>>
    where
        anyhow::Error: From<E>,
    {
        let Ok(val) = result;
        Ok(val)
    }

    fn map_err<T, E1, E2>(
        &self,
        result: Result<T, Self::Error<E1>>,
        _map_err: impl FnOnce(E1) -> E2,
    ) -> Result<T, Self::Error<E2>> {
        let Ok(val) = result;
        Ok(val)
    }
}

/// A [`MisconfigurationStrategy`] that happily fails whenever
/// an important `Err` is encountered.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct FailStrategy;

impl MisconfigurationStrategy for FailStrategy {
    type Error<E> = E;

    fn fallback<T, E>(
        &self,
        result: Result<T, E>,
        _fallback_fn: impl FnOnce(E) -> T,
    ) -> Result<T, Self::Error<E>> {
        result
    }

    fn fallback_opt<T, E>(
        &self,
        result: Result<T, E>,
        _fallback_fn: impl FnOnce(E),
    ) -> Result<Option<T>, Self::Error<E>> {
        result.map(Some)
    }

    fn to_anyhow<T, E>(
        &self,
        result: Result<T, Self::Error<E>>,
    ) -> Result<T, Self::Error<anyhow::Error>>
    where
        anyhow::Error: From<E>,
    {
        Ok(result?)
    }

    fn map_err<T, E1, E2>(
        &self,
        result: Result<T, Self::Error<E1>>,
        map_err: impl FnOnce(E1) -> E2,
    ) -> Result<T, Self::Error<E2>> {
        result.map_err(map_err)
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

    pub fn to_search_paths<Strategy: MisconfigurationStrategy>(
        &self,
        system: &dyn System,
        vendored: &VendoredFileSystem,
        strategy: &Strategy,
    ) -> Result<SearchPaths, Strategy::Error<SearchPathValidationError>> {
        SearchPaths::from_settings(self, system, vendored, strategy)
    }
}
