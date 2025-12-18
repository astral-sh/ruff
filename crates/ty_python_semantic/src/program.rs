use crate::Db;
use crate::python_platform::PythonPlatform;

use ruff_db::system::SystemPath;
use ruff_python_ast::PythonVersion;
use salsa::Durability;
use salsa::Setter;
use ty_module_resolver::SearchPaths;
use ty_site_packages::PythonVersionWithSource;

#[salsa::input(singleton, heap_size=ruff_memory_usage::heap_size)]
pub struct Program {
    #[returns(ref)]
    pub python_version_with_source: PythonVersionWithSource,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(ref)]
    pub search_paths: SearchPaths,
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

    /// Convenience to convert the inner `Error` to `anyhow::Error`.
    fn to_anyhow<T, E>(
        &self,
        result: Result<T, Self::Error<E>>,
    ) -> Result<T, Self::Error<anyhow::Error>>
    where
        anyhow::Error: From<E>;

    /// Convenience to map the inner `Error`.
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
