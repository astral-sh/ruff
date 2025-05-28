//! Utilities for finding the `site-packages` directory,
//! into which third-party packages are installed.
//!
//! The routines exposed by this module have different behaviour depending
//! on the platform of the *host machine*, which may be
//! different from the *target platform for type checking*. (A user
//! might be running ty on a Windows machine, but might
//! reasonably ask us to type-check code assuming that the code runs
//! on Linux.)

use std::fmt;
use std::fmt::Display;
use std::io;
use std::num::NonZeroUsize;
use std::ops::Deref;

use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;

type SitePackagesDiscoveryResult<T> = Result<T, SitePackagesDiscoveryError>;

#[derive(Debug)]
pub(crate) enum PythonEnvironment {
    Virtual(VirtualEnvironment),
    System(SystemEnvironment),
}

impl PythonEnvironment {
    pub(crate) fn new(
        path: impl AsRef<SystemPath>,
        origin: SysPrefixPathOrigin,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Self> {
        let path = SysPrefixPath::new(path, origin, system)?;

        // Attempt to inspect as a virtual environment first
        // TODO(zanieb): Consider avoiding the clone here by checking for `pyvenv.cfg` ahead-of-time
        match VirtualEnvironment::new(path.clone(), origin, system) {
            Ok(venv) => Ok(Self::Virtual(venv)),
            // If there's not a `pyvenv.cfg` marker, attempt to inspect as a system environment
            //
            Err(SitePackagesDiscoveryError::NoPyvenvCfgFile(_, _))
                if !origin.must_be_virtual_env() =>
            {
                Ok(Self::System(SystemEnvironment::new(path)))
            }
            Err(err) => Err(err),
        }
    }

    /// Returns the `site-packages` directories for this Python environment.
    ///
    /// See the documentation for [`site_packages_directory_from_sys_prefix`] for more details.
    pub(crate) fn site_packages_directories(
        &self,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Vec<SystemPathBuf>> {
        match self {
            Self::Virtual(env) => env.site_packages_directories(system),
            Self::System(env) => env.site_packages_directories(system),
        }
    }
}

/// The Python runtime that produced the venv.
///
/// We only need to distinguish cases that change the on-disk layout.
/// Everything else can be treated like CPython.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum PythonImplementation {
    CPython,
    PyPy,
    GraalPy,
    /// Fallback when the value is missing or unrecognised.
    /// We treat it like CPython but keep the information for diagnostics.
    Unknown,
}

impl PythonImplementation {
    /// Return the relative path from `sys.prefix` to the `site-packages` directory
    /// if this is a known implementation. Return `None` if this is an unknown implementation.
    fn relative_site_packages_path(self, version: Option<PythonVersion>) -> Option<String> {
        match self {
            Self::CPython | Self::GraalPy => {
                version.map(|version| format!("lib/python{version}/site-packages"))
            }
            Self::PyPy => version.map(|version| format!("lib/pypy{version}/site-packages")),
            Self::Unknown => None,
        }
    }
}

/// Abstraction for a Python virtual environment.
///
/// Most of this information is derived from the virtual environment's `pyvenv.cfg` file.
/// The format of this file is not defined anywhere, and exactly which keys are present
/// depends on the tool that was used to create the virtual environment.
#[derive(Debug)]
pub(crate) struct VirtualEnvironment {
    root_path: SysPrefixPath,
    base_executable_home_path: PythonHomePath,
    include_system_site_packages: bool,

    /// The version of the Python executable that was used to create this virtual environment.
    ///
    /// The Python version is encoded under different keys and in different formats
    /// by different virtual-environment creation tools,
    /// and the key is never read by the standard-library `site.py` module,
    /// so it's possible that we might not be able to find this information
    /// in an acceptable format under any of the keys we expect.
    /// This field will be `None` if so.
    version: Option<PythonVersion>,
    implementation: PythonImplementation,
}

impl VirtualEnvironment {
    pub(crate) fn new(
        path: SysPrefixPath,
        origin: SysPrefixPathOrigin,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Self> {
        fn pyvenv_cfg_line_number(index: usize) -> NonZeroUsize {
            index.checked_add(1).and_then(NonZeroUsize::new).unwrap()
        }

        let pyvenv_cfg_path = path.join("pyvenv.cfg");
        tracing::debug!("Attempting to parse virtual environment metadata at '{pyvenv_cfg_path}'");

        let pyvenv_cfg = system
            .read_to_string(&pyvenv_cfg_path)
            .map_err(|io_err| SitePackagesDiscoveryError::NoPyvenvCfgFile(origin, io_err))?;

        let mut include_system_site_packages = false;
        let mut base_executable_home_path = None;
        let mut version_info_string = None;
        let mut implementation = PythonImplementation::Unknown;

        // A `pyvenv.cfg` file *looks* like a `.ini` file, but actually isn't valid `.ini` syntax!
        // The Python standard-library's `site` module parses these files by splitting each line on
        // '=' characters, so that's what we should do as well.
        //
        // See also: https://snarky.ca/how-virtual-environments-work/
        for (index, line) in pyvenv_cfg.lines().enumerate() {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if key.is_empty() {
                    return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                        pyvenv_cfg_path,
                        PyvenvCfgParseErrorKind::MalformedKeyValuePair {
                            line_number: pyvenv_cfg_line_number(index),
                        },
                    ));
                }

                let value = value.trim();
                if value.is_empty() {
                    return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                        pyvenv_cfg_path,
                        PyvenvCfgParseErrorKind::MalformedKeyValuePair {
                            line_number: pyvenv_cfg_line_number(index),
                        },
                    ));
                }

                match key {
                    "include-system-site-packages" => {
                        include_system_site_packages = value.eq_ignore_ascii_case("true");
                    }
                    "home" => base_executable_home_path = Some(value),
                    // `virtualenv` and `uv` call this key `version_info`,
                    // but the stdlib venv module calls it `version`
                    "version" | "version_info" => version_info_string = Some(value),
                    "implementation" => {
                        implementation = match value.to_ascii_lowercase().as_str() {
                            "cpython" => PythonImplementation::CPython,
                            "graalvm" => PythonImplementation::GraalPy,
                            "pypy" => PythonImplementation::PyPy,
                            _ => PythonImplementation::Unknown,
                        };
                    }
                    _ => continue,
                }
            }
        }

        // The `home` key is read by the standard library's `site.py` module,
        // so if it's missing from the `pyvenv.cfg` file
        // (or the provided value is invalid),
        // it's reasonable to consider the virtual environment irredeemably broken.
        let Some(base_executable_home_path) = base_executable_home_path else {
            return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                pyvenv_cfg_path,
                PyvenvCfgParseErrorKind::NoHomeKey,
            ));
        };
        let base_executable_home_path = PythonHomePath::new(base_executable_home_path, system)
            .map_err(|io_err| {
                SitePackagesDiscoveryError::PyvenvCfgParseError(
                    pyvenv_cfg_path,
                    PyvenvCfgParseErrorKind::InvalidHomeValue(io_err),
                )
            })?;

        // but the `version`/`version_info` key is not read by the standard library,
        // and is provided under different keys depending on which virtual-environment creation tool
        // created the `pyvenv.cfg` file. Lenient parsing is appropriate here:
        // the file isn't really *invalid* if it doesn't have this key,
        // or if the value doesn't parse according to our expectations.
        let version = version_info_string.and_then(|version_string| {
            let mut version_info_parts = version_string.split('.');
            let (major, minor) = (version_info_parts.next()?, version_info_parts.next()?);
            PythonVersion::try_from((major, minor)).ok()
        });

        let metadata = Self {
            root_path: path,
            base_executable_home_path,
            include_system_site_packages,
            version,
            implementation,
        };

        tracing::trace!("Resolved metadata for virtual environment: {metadata:?}");
        Ok(metadata)
    }

    /// Return a list of `site-packages` directories that are available from this virtual environment
    ///
    /// See the documentation for [`site_packages_directory_from_sys_prefix`] for more details.
    pub(crate) fn site_packages_directories(
        &self,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Vec<SystemPathBuf>> {
        let VirtualEnvironment {
            root_path,
            base_executable_home_path,
            include_system_site_packages,
            implementation,
            version,
        } = self;

        let mut site_packages_directories = vec![site_packages_directory_from_sys_prefix(
            root_path,
            *version,
            *implementation,
            system,
        )?];

        if *include_system_site_packages {
            let system_sys_prefix =
                SysPrefixPath::from_executable_home_path(base_executable_home_path);

            // If we fail to resolve the `sys.prefix` path from the base executable home path,
            // or if we fail to resolve the `site-packages` from the `sys.prefix` path,
            // we should probably print a warning but *not* abort type checking
            if let Some(sys_prefix_path) = system_sys_prefix {
                match site_packages_directory_from_sys_prefix(
                    &sys_prefix_path,
                    *version,
                    *implementation,
                    system,
                ) {
                    Ok(site_packages_directory) => {
                        site_packages_directories.push(site_packages_directory);
                    }
                    Err(error) => tracing::warn!(
                        "{error}. System site-packages will not be used for module resolution."
                    ),
                }
            } else {
                tracing::warn!(
                    "Failed to resolve `sys.prefix` of the system Python installation \
from the `home` value in the `pyvenv.cfg` file at `{}`. \
System site-packages will not be used for module resolution.",
                    root_path.join("pyvenv.cfg")
                );
            }
        }

        tracing::debug!(
            "Resolved site-packages directories for this virtual environment are: {site_packages_directories:?}"
        );
        Ok(site_packages_directories)
    }
}

/// A Python environment that is _not_ a virtual environment.
///
/// This environment may or may not be one that is managed by the operating system itself, e.g.,
/// this captures both Homebrew-installed Python versions and the bundled macOS Python installation.
#[derive(Debug)]
pub(crate) struct SystemEnvironment {
    root_path: SysPrefixPath,
}

impl SystemEnvironment {
    /// Create a new system environment from the given path.
    ///
    /// At this time, there is no eager validation and this is infallible. Instead, validation
    /// will occur in [`site_packages_directory_from_sys_prefix`] — which will fail if there is not
    /// a Python environment at the given path.
    pub(crate) fn new(path: SysPrefixPath) -> Self {
        Self { root_path: path }
    }

    /// Return a list of `site-packages` directories that are available from this environment.
    ///
    /// See the documentation for [`site_packages_directory_from_sys_prefix`] for more details.
    pub(crate) fn site_packages_directories(
        &self,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Vec<SystemPathBuf>> {
        let SystemEnvironment { root_path } = self;

        let site_packages_directories = vec![site_packages_directory_from_sys_prefix(
            root_path,
            None,
            PythonImplementation::Unknown,
            system,
        )?];

        tracing::debug!(
            "Resolved site-packages directories for this environment are: {site_packages_directories:?}"
        );
        Ok(site_packages_directories)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SitePackagesDiscoveryError {
    #[error("Invalid {1}: `{0}` could not be canonicalized")]
    EnvDirCanonicalizationError(SystemPathBuf, SysPrefixPathOrigin, #[source] io::Error),
    #[error("Invalid {1}: `{0}` does not point to a directory on disk")]
    EnvDirNotDirectory(SystemPathBuf, SysPrefixPathOrigin),
    #[error("{0} points to a broken venv with no pyvenv.cfg file")]
    NoPyvenvCfgFile(SysPrefixPathOrigin, #[source] io::Error),
    #[error("Failed to parse the pyvenv.cfg file at {0} because {1}")]
    PyvenvCfgParseError(SystemPathBuf, PyvenvCfgParseErrorKind),
    #[error(
        "Failed to search the `lib` directory of the Python installation at {1} for `site-packages`"
    )]
    CouldNotReadLibDirectory(#[source] io::Error, SysPrefixPath),
    #[error("Could not find the `site-packages` directory for the Python installation at {0}")]
    NoSitePackagesDirFound(SysPrefixPath),
}

/// The various ways in which parsing a `pyvenv.cfg` file could fail
#[derive(Debug)]
pub(crate) enum PyvenvCfgParseErrorKind {
    MalformedKeyValuePair { line_number: NonZeroUsize },
    NoHomeKey,
    InvalidHomeValue(io::Error),
}

impl fmt::Display for PyvenvCfgParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedKeyValuePair { line_number } => write!(
                f,
                "line {line_number} has a malformed `<key> = <value>` pair"
            ),
            Self::NoHomeKey => f.write_str("the file does not have a `home` key"),
            Self::InvalidHomeValue(io_err) => {
                write!(
                    f,
                    "the following error was encountered \
when trying to resolve the `home` value to a directory on disk: {io_err}"
                )
            }
        }
    }
}

/// Attempt to retrieve the `site-packages` directory
/// associated with a given Python installation.
///
/// The location of the `site-packages` directory can vary according to the
/// Python version that this installation represents. The Python version may
/// or may not be known at this point, which is why the `python_version`
/// parameter is an `Option`.
fn site_packages_directory_from_sys_prefix(
    sys_prefix_path: &SysPrefixPath,
    python_version: Option<PythonVersion>,
    implementation: PythonImplementation,
    system: &dyn System,
) -> SitePackagesDiscoveryResult<SystemPathBuf> {
    tracing::debug!("Searching for site-packages directory in {sys_prefix_path}");

    if cfg!(target_os = "windows") {
        let site_packages = sys_prefix_path.join(r"Lib\site-packages");
        return system
            .is_directory(&site_packages)
            .then_some(site_packages)
            .ok_or(SitePackagesDiscoveryError::NoSitePackagesDirFound(
                sys_prefix_path.to_owned(),
            ));
    }

    // In the Python standard library's `site.py` module (used for finding `site-packages`
    // at runtime), we can find this in [the non-Windows branch]:
    //
    // ```py
    // libdirs = [sys.platlibdir]
    // if sys.platlibdir != "lib":
    //     libdirs.append("lib")
    // ```
    //
    // Pyright therefore searches for both a `lib/python3.X/site-packages` directory
    // and a `lib64/python3.X/site-packages` directory on non-MacOS Unix systems,
    // since `sys.platlibdir` can sometimes be equal to `"lib64"`.
    //
    // However, we only care about the `site-packages` directory insofar as it allows
    // us to discover Python source code that can be used for inferring type
    // information regarding third-party dependencies. That means that we don't need
    // to care about any possible `lib64/site-packages` directories, since
    // [the `sys`-module documentation] states that `sys.platlibdir` is *only* ever
    // used for C extensions, never for pure-Python modules.
    //
    // [the non-Windows branch]: https://github.com/python/cpython/blob/a8be8fc6c4682089be45a87bd5ee1f686040116c/Lib/site.py#L401-L410
    // [the `sys`-module documentation]: https://docs.python.org/3/library/sys.html#sys.platlibdir

    // If we were able to figure out what Python version this installation is,
    // we should be able to avoid iterating through all items in the `lib/` directory:
    if let Some(expected_relative_path) = implementation.relative_site_packages_path(python_version)
    {
        let expected_absolute_path = sys_prefix_path.join(expected_relative_path);
        if system.is_directory(&expected_absolute_path) {
            return Ok(expected_absolute_path);
        }

        // CPython free-threaded (3.13+) variant: pythonXYt
        if matches!(implementation, PythonImplementation::CPython)
            && python_version.is_some_and(PythonVersion::free_threaded_build_available)
        {
            let alternative_path = sys_prefix_path.join(format!(
                "lib/python{}t/site-packages",
                python_version.unwrap()
            ));
            if system.is_directory(&alternative_path) {
                return Ok(alternative_path);
            }
        }
    }

    // Either we couldn't figure out the version before calling this function
    // (e.g., from a `pyvenv.cfg` file if this was a venv),
    // or we couldn't find a `site-packages` folder at the expected location given
    // the parsed version
    //
    // Note: the `python3.x` part of the `site-packages` path can't be computed from
    // the `--python-version` the user has passed, as they might be running Python 3.12 locally
    // even if they've requested that we type check their code "as if" they're running 3.8.
    for entry_result in system
        .read_directory(&sys_prefix_path.join("lib"))
        .map_err(|io_err| {
            SitePackagesDiscoveryError::CouldNotReadLibDirectory(io_err, sys_prefix_path.to_owned())
        })?
    {
        let Ok(entry) = entry_result else {
            continue;
        };

        if !entry.file_type().is_directory() {
            continue;
        }

        let mut path = entry.into_path();

        let name = path
            .file_name()
            .expect("File name to be non-null because path is guaranteed to be a child of `lib`");

        if !(name.starts_with("python3.") || name.starts_with("pypy3.")) {
            continue;
        }

        path.push("site-packages");
        if system.is_directory(&path) {
            return Ok(path);
        }
    }
    Err(SitePackagesDiscoveryError::NoSitePackagesDirFound(
        sys_prefix_path.to_owned(),
    ))
}

/// A path that represents the value of [`sys.prefix`] at runtime in Python
/// for a given Python executable.
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
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct SysPrefixPath {
    inner: SystemPathBuf,
    origin: SysPrefixPathOrigin,
}

impl SysPrefixPath {
    fn new(
        unvalidated_path: impl AsRef<SystemPath>,
        origin: SysPrefixPathOrigin,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Self> {
        Self::new_impl(unvalidated_path.as_ref(), origin, system)
    }

    fn new_impl(
        unvalidated_path: &SystemPath,
        origin: SysPrefixPathOrigin,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Self> {
        // It's important to resolve symlinks here rather than simply making the path absolute,
        // since system Python installations often only put symlinks in the "expected"
        // locations for `home` and `site-packages`
        let canonicalized = system
            .canonicalize_path(unvalidated_path)
            .map_err(|io_err| {
                SitePackagesDiscoveryError::EnvDirCanonicalizationError(
                    unvalidated_path.to_path_buf(),
                    origin,
                    io_err,
                )
            })?;
        system
            .is_directory(&canonicalized)
            .then_some(Self {
                inner: canonicalized,
                origin,
            })
            .ok_or_else(|| {
                SitePackagesDiscoveryError::EnvDirNotDirectory(
                    unvalidated_path.to_path_buf(),
                    origin,
                )
            })
    }

    fn from_executable_home_path(path: &PythonHomePath) -> Option<Self> {
        // No need to check whether `path.parent()` is a directory:
        // the parent of a canonicalised path that is known to exist
        // is guaranteed to be a directory.
        if cfg!(target_os = "windows") {
            Some(Self {
                inner: path.to_path_buf(),
                origin: SysPrefixPathOrigin::Derived,
            })
        } else {
            path.parent().map(|path| Self {
                inner: path.to_path_buf(),
                origin: SysPrefixPathOrigin::Derived,
            })
        }
    }
}

impl Deref for SysPrefixPath {
    type Target = SystemPath;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl fmt::Display for SysPrefixPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`sys.prefix` path `{}`", self.inner)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SysPrefixPathOrigin {
    PythonCliFlag,
    VirtualEnvVar,
    CondaPrefixVar,
    Derived,
    LocalVenv,
}

impl SysPrefixPathOrigin {
    /// Whether the given `sys.prefix` path must be a virtual environment (rather than a system
    /// Python environment).
    pub(crate) fn must_be_virtual_env(self) -> bool {
        match self {
            Self::LocalVenv | Self::VirtualEnvVar => true,
            Self::PythonCliFlag | Self::Derived | Self::CondaPrefixVar => false,
        }
    }
}

impl Display for SysPrefixPathOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::PythonCliFlag => f.write_str("`--python` argument"),
            Self::VirtualEnvVar => f.write_str("`VIRTUAL_ENV` environment variable"),
            Self::CondaPrefixVar => f.write_str("`CONDA_PREFIX` environment variable"),
            Self::Derived => f.write_str("derived `sys.prefix` path"),
            Self::LocalVenv => f.write_str("local virtual environment"),
        }
    }
}

/// The value given by the `home` key in `pyvenv.cfg` files.
///
/// This is equivalent to `{sys_prefix_path}/bin`, and points
/// to a directory in which a Python executable can be found.
/// Confusingly, it is *not* the same as the [`PYTHONHOME`]
/// environment variable that Python provides! However, it's
/// consistent among all mainstream creators of Python virtual
/// environments (the stdlib Python `venv` module, the third-party
/// `virtualenv` library, and `uv`), was specified by
/// [the original PEP adding the `venv` module],
/// and it's one of the few fields that's read by the Python
/// standard library's `site.py` module.
///
/// Although it doesn't appear to be specified anywhere,
/// all existing virtual environment tools always use an absolute path
/// for the `home` value, and the Python standard library also assumes
/// that the `home` value will be an absolute path.
///
/// Other values, such as the path to the Python executable or the
/// base-executable `sys.prefix` value, are either only provided in
/// `pyvenv.cfg` files by some virtual-environment creators,
/// or are included under different keys depending on which
/// virtual-environment creation tool you've used.
///
/// [`PYTHONHOME`]: https://docs.python.org/3/using/cmdline.html#envvar-PYTHONHOME
/// [the original PEP adding the `venv` module]: https://peps.python.org/pep-0405/
#[derive(Debug, PartialEq, Eq)]
struct PythonHomePath(SystemPathBuf);

impl PythonHomePath {
    fn new(path: impl AsRef<SystemPath>, system: &dyn System) -> io::Result<Self> {
        let path = path.as_ref();
        // It's important to resolve symlinks here rather than simply making the path absolute,
        // since system Python installations often only put symlinks in the "expected"
        // locations for `home` and `site-packages`
        let canonicalized = system.canonicalize_path(path)?;
        system
            .is_directory(&canonicalized)
            .then_some(Self(canonicalized))
            .ok_or_else(|| io::Error::other("not a directory"))
    }
}

impl Deref for PythonHomePath {
    type Target = SystemPath;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for PythonHomePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`home` location `{}`", self.0)
    }
}

impl PartialEq<SystemPath> for PythonHomePath {
    fn eq(&self, other: &SystemPath) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<SystemPathBuf> for PythonHomePath {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self == &**other
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::system::TestSystem;

    use super::*;

    impl PythonEnvironment {
        fn expect_venv(self) -> VirtualEnvironment {
            match self {
                Self::Virtual(venv) => venv,
                Self::System(_) => panic!("Expected a virtual environment"),
            }
        }
    }

    struct VirtualEnvironmentTestCase {
        system_site_packages: bool,
        pyvenv_cfg_version_field: Option<&'static str>,
        command_field: Option<&'static str>,
        implementation_field: Option<&'static str>,
    }

    struct PythonEnvironmentTestCase {
        system: TestSystem,
        minor_version: u8,
        free_threaded: bool,
        origin: SysPrefixPathOrigin,
        virtual_env: Option<VirtualEnvironmentTestCase>,
    }

    impl PythonEnvironmentTestCase {
        /// Builds a mock environment, and returns the path to the environment root.
        fn build(&self) -> SystemPathBuf {
            let PythonEnvironmentTestCase {
                system,
                minor_version,
                free_threaded,
                origin: _,
                virtual_env,
            } = self;
            let memory_fs = system.memory_file_system();
            let unix_site_packages = if *free_threaded {
                format!("lib/python3.{minor_version}t/site-packages")
            } else {
                format!("lib/python3.{minor_version}/site-packages")
            };

            let system_install_sys_prefix =
                SystemPathBuf::from(&*format!("/Python3.{minor_version}"));
            let (system_home_path, system_exe_path, system_site_packages_path) =
                if cfg!(target_os = "windows") {
                    let system_home_path = system_install_sys_prefix.clone();
                    let system_exe_path = system_home_path.join("python.exe");
                    let system_site_packages_path =
                        system_install_sys_prefix.join(r"Lib\site-packages");
                    (system_home_path, system_exe_path, system_site_packages_path)
                } else {
                    let system_home_path = system_install_sys_prefix.join("bin");
                    let system_exe_path = system_home_path.join("python");
                    let system_site_packages_path =
                        system_install_sys_prefix.join(&unix_site_packages);
                    (system_home_path, system_exe_path, system_site_packages_path)
                };
            memory_fs.write_file_all(system_exe_path, "").unwrap();
            memory_fs
                .create_directory_all(&system_site_packages_path)
                .unwrap();

            let Some(VirtualEnvironmentTestCase {
                pyvenv_cfg_version_field,
                system_site_packages,
                command_field,
                implementation_field,
            }) = virtual_env
            else {
                return system_install_sys_prefix;
            };

            let venv_sys_prefix = SystemPathBuf::from("/.venv");
            let (venv_exe, site_packages_path) = if cfg!(target_os = "windows") {
                (
                    venv_sys_prefix.join(r"Scripts\python.exe"),
                    venv_sys_prefix.join(r"Lib\site-packages"),
                )
            } else {
                (
                    venv_sys_prefix.join("bin/python"),
                    venv_sys_prefix.join(&unix_site_packages),
                )
            };
            memory_fs.write_file_all(&venv_exe, "").unwrap();
            memory_fs.create_directory_all(&site_packages_path).unwrap();

            let pyvenv_cfg_path = venv_sys_prefix.join("pyvenv.cfg");
            let mut pyvenv_cfg_contents = format!("home = {system_home_path}\n");
            if let Some(version_field) = pyvenv_cfg_version_field {
                pyvenv_cfg_contents.push_str(version_field);
                pyvenv_cfg_contents.push('\n');
            }
            if let Some(command_field) = command_field {
                pyvenv_cfg_contents.push_str(command_field);
                pyvenv_cfg_contents.push('\n');
            }
            if let Some(implementation_field) = implementation_field {
                pyvenv_cfg_contents.push_str(implementation_field);
                pyvenv_cfg_contents.push('\n');
            }
            // Deliberately using weird casing here to test that our pyvenv.cfg parsing is case-insensitive:
            if *system_site_packages {
                pyvenv_cfg_contents.push_str("include-system-site-packages = TRuE\n");
            }
            memory_fs
                .write_file_all(pyvenv_cfg_path, &pyvenv_cfg_contents)
                .unwrap();

            venv_sys_prefix
        }

        #[track_caller]
        fn err(self) -> SitePackagesDiscoveryError {
            PythonEnvironment::new(self.build(), self.origin, &self.system)
                .expect_err("Expected environment construction to fail")
        }

        #[track_caller]
        fn run(self) -> PythonEnvironment {
            let env_path = self.build();
            let env = PythonEnvironment::new(env_path.clone(), self.origin, &self.system)
                .expect("Expected environment construction to succeed");

            let expect_virtual_env = self.virtual_env.is_some();
            match &env {
                PythonEnvironment::Virtual(venv) if expect_virtual_env => {
                    self.assert_virtual_environment(venv, &env_path);
                }
                PythonEnvironment::Virtual(venv) => {
                    panic!(
                        "Expected a system environment, but got a virtual environment: {venv:?}"
                    );
                }
                PythonEnvironment::System(env) if !expect_virtual_env => {
                    self.assert_system_environment(env, &env_path);
                }
                PythonEnvironment::System(env) => {
                    panic!("Expected a virtual environment, but got a system environment: {env:?}");
                }
            }
            env
        }

        fn assert_virtual_environment(
            &self,
            venv: &VirtualEnvironment,
            expected_env_path: &SystemPathBuf,
        ) {
            let self_venv = self.virtual_env.as_ref().expect(
                "`assert_virtual_environment` should only be used when `virtual_env` is populated",
            );

            assert_eq!(
                venv.root_path,
                SysPrefixPath {
                    inner: self.system.canonicalize_path(expected_env_path).unwrap(),
                    origin: self.origin,
                }
            );
            assert_eq!(
                venv.include_system_site_packages,
                self_venv.system_site_packages
            );

            if self_venv.pyvenv_cfg_version_field.is_some() {
                assert_eq!(
                    venv.version,
                    Some(PythonVersion {
                        major: 3,
                        minor: self.minor_version
                    })
                );
            } else {
                assert_eq!(venv.version, None);
            }

            let expected_home = if cfg!(target_os = "windows") {
                SystemPathBuf::from(&*format!(r"\Python3.{}", self.minor_version))
            } else {
                SystemPathBuf::from(&*format!("/Python3.{}/bin", self.minor_version))
            };
            assert_eq!(venv.base_executable_home_path, expected_home);

            let site_packages_directories = venv.site_packages_directories(&self.system).unwrap();
            let expected_venv_site_packages = if cfg!(target_os = "windows") {
                SystemPathBuf::from(r"\.venv\Lib\site-packages")
            } else if self.free_threaded {
                SystemPathBuf::from(&*format!(
                    "/.venv/lib/python3.{}t/site-packages",
                    self.minor_version
                ))
            } else {
                SystemPathBuf::from(&*format!(
                    "/.venv/lib/python3.{}/site-packages",
                    self.minor_version
                ))
            };

            let expected_system_site_packages = if cfg!(target_os = "windows") {
                SystemPathBuf::from(&*format!(
                    r"\Python3.{}\Lib\site-packages",
                    self.minor_version
                ))
            } else if self.free_threaded {
                SystemPathBuf::from(&*format!(
                    "/Python3.{minor_version}/lib/python3.{minor_version}t/site-packages",
                    minor_version = self.minor_version
                ))
            } else {
                SystemPathBuf::from(&*format!(
                    "/Python3.{minor_version}/lib/python3.{minor_version}/site-packages",
                    minor_version = self.minor_version
                ))
            };

            if self_venv.system_site_packages {
                assert_eq!(
                    &site_packages_directories,
                    &[expected_venv_site_packages, expected_system_site_packages]
                );
            } else {
                assert_eq!(&site_packages_directories, &[expected_venv_site_packages]);
            }
        }

        fn assert_system_environment(
            &self,
            env: &SystemEnvironment,
            expected_env_path: &SystemPathBuf,
        ) {
            assert!(
                self.virtual_env.is_none(),
                "`assert_system_environment` should only be used when `virtual_env` is not populated"
            );

            assert_eq!(
                env.root_path,
                SysPrefixPath {
                    inner: self.system.canonicalize_path(expected_env_path).unwrap(),
                    origin: self.origin,
                }
            );

            let site_packages_directories = env.site_packages_directories(&self.system).unwrap();

            let expected_site_packages = if cfg!(target_os = "windows") {
                SystemPathBuf::from(&*format!(
                    r"\Python3.{}\Lib\site-packages",
                    self.minor_version
                ))
            } else if self.free_threaded {
                SystemPathBuf::from(&*format!(
                    "/Python3.{minor_version}/lib/python3.{minor_version}t/site-packages",
                    minor_version = self.minor_version
                ))
            } else {
                SystemPathBuf::from(&*format!(
                    "/Python3.{minor_version}/lib/python3.{minor_version}/site-packages",
                    minor_version = self.minor_version
                ))
            };

            assert_eq!(&site_packages_directories, &[expected_site_packages]);
        }
    }

    #[test]
    fn can_find_site_packages_directory_no_virtual_env() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::PythonCliFlag,
            virtual_env: None,
        };
        test.run();
    }

    #[test]
    fn can_find_site_packages_directory_no_virtual_env_freethreaded() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::PythonCliFlag,
            virtual_env: None,
        };
        test.run();
    }

    #[test]
    fn cannot_find_site_packages_directory_no_virtual_env_at_origin_virtual_env_var() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: None,
        };
        let err = test.err();
        assert!(
            matches!(err, SitePackagesDiscoveryError::NoPyvenvCfgFile(..)),
            "Got {err:?}",
        );
    }

    #[test]
    fn cannot_find_site_packages_directory_no_virtual_env_at_origin_local_venv() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: false,
            origin: SysPrefixPathOrigin::LocalVenv,
            virtual_env: None,
        };
        let err = test.err();
        assert!(
            matches!(err, SitePackagesDiscoveryError::NoPyvenvCfgFile(..)),
            "Got {err:?}",
        );
    }

    #[test]
    fn can_find_site_packages_directory_no_version_field_in_pyvenv_cfg() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: false,
                pyvenv_cfg_version_field: None,
                command_field: None,
                implementation_field: None,
            }),
        };
        test.run();
    }

    #[test]
    fn can_find_site_packages_directory_venv_style_version_field_in_pyvenv_cfg() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: false,
                pyvenv_cfg_version_field: Some("version = 3.12"),
                command_field: None,
                implementation_field: None,
            }),
        };
        test.run();
    }

    #[test]
    fn can_find_site_packages_directory_uv_style_version_field_in_pyvenv_cfg() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: false,
                pyvenv_cfg_version_field: Some("version_info = 3.12"),
                command_field: None,
                implementation_field: None,
            }),
        };
        test.run();
    }

    #[test]
    fn can_find_site_packages_directory_virtualenv_style_version_field_in_pyvenv_cfg() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: false,
                pyvenv_cfg_version_field: Some("version_info = 3.12.0rc2"),
                command_field: None,
                implementation_field: None,
            }),
        };
        test.run();
    }

    #[test]
    fn can_find_site_packages_directory_freethreaded_build() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: false,
                pyvenv_cfg_version_field: Some("version_info = 3.13"),
                command_field: None,
                implementation_field: None,
            }),
        };
        test.run();
    }

    #[test]
    fn finds_system_site_packages() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: true,
                pyvenv_cfg_version_field: Some("version_info = 3.13"),
                command_field: None,
                implementation_field: None,
            }),
        };
        test.run();
    }

    #[test]
    fn detects_pypy_implementation() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: true,
                pyvenv_cfg_version_field: None,
                command_field: None,
                implementation_field: Some("implementation = PyPy"),
            }),
        };
        let venv = test.run().expect_venv();
        assert_eq!(venv.implementation, PythonImplementation::PyPy);
    }

    #[test]
    fn detects_cpython_implementation() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: true,
                pyvenv_cfg_version_field: None,
                command_field: None,
                implementation_field: Some("implementation = CPython"),
            }),
        };
        let venv = test.run().expect_venv();
        assert_eq!(venv.implementation, PythonImplementation::CPython);
    }

    #[test]
    fn detects_graalpy_implementation() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: true,
                pyvenv_cfg_version_field: None,
                command_field: None,
                implementation_field: Some("implementation = GraalVM"),
            }),
        };
        let venv = test.run().expect_venv();
        assert_eq!(venv.implementation, PythonImplementation::GraalPy);
    }

    #[test]
    fn detects_unknown_implementation() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: true,
                pyvenv_cfg_version_field: None,
                command_field: None,
                implementation_field: None,
            }),
        };
        let venv = test.run().expect_venv();
        assert_eq!(venv.implementation, PythonImplementation::Unknown);
    }

    #[test]
    fn reject_env_that_does_not_exist() {
        let system = TestSystem::default();
        assert!(matches!(
            PythonEnvironment::new("/env", SysPrefixPathOrigin::PythonCliFlag, &system),
            Err(SitePackagesDiscoveryError::EnvDirCanonicalizationError(..))
        ));
    }

    #[test]
    fn reject_env_that_is_not_a_directory() {
        let system = TestSystem::default();
        system
            .memory_file_system()
            .write_file_all("/env", "")
            .unwrap();
        assert!(matches!(
            PythonEnvironment::new("/env", SysPrefixPathOrigin::PythonCliFlag, &system),
            Err(SitePackagesDiscoveryError::EnvDirNotDirectory(..))
        ));
    }

    #[test]
    fn cannot_read_lib_directory() {
        let system = TestSystem::default();
        system
            .memory_file_system()
            .create_directory_all("/env")
            .unwrap();
        // Environment creation succeeds, but site-packages retrieval fails reading the `lib`
        // directory
        let env =
            PythonEnvironment::new("/env", SysPrefixPathOrigin::PythonCliFlag, &system).unwrap();
        let site_packages = env.site_packages_directories(&system);
        if cfg!(unix) {
            assert!(
                matches!(
                    site_packages,
                    Err(SitePackagesDiscoveryError::CouldNotReadLibDirectory(..)),
                ),
                "Got {site_packages:?}",
            );
        } else {
            // On Windows, we look for `Lib/site-packages` directly instead of listing the entries
            // of `lib/...` — so we don't see the intermediate failure
            assert!(
                matches!(
                    site_packages,
                    Err(SitePackagesDiscoveryError::NoSitePackagesDirFound(..)),
                ),
                "Got {site_packages:?}",
            );
        }
    }

    #[test]
    fn cannot_find_site_packages_directory() {
        let system = TestSystem::default();
        if cfg!(unix) {
            system
                .memory_file_system()
                .create_directory_all("/env/lib")
                .unwrap();
        } else {
            system
                .memory_file_system()
                .create_directory_all("/env/Lib")
                .unwrap();
        }
        // Environment creation succeeds, but site-packages retrieval fails
        let env =
            PythonEnvironment::new("/env", SysPrefixPathOrigin::PythonCliFlag, &system).unwrap();
        let site_packages = env.site_packages_directories(&system);
        assert!(
            matches!(
                site_packages,
                Err(SitePackagesDiscoveryError::NoSitePackagesDirFound(..)),
            ),
            "Got {site_packages:?}",
        );
    }

    /// See <https://github.com/astral-sh/ty/issues/430>
    #[test]
    fn parsing_pyvenv_cfg_with_equals_in_value() {
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: true,
                pyvenv_cfg_version_field: Some("version_info = 3.13"),
                command_field: Some(
                    r#"command = /.pyenv/versions/3.13.3/bin/python3.13 -m venv --without-pip --prompt="python-default/3.13.3" /somewhere-else/python/virtualenvs/python-default/3.13.3"#,
                ),
                implementation_field: None,
            }),
        };
        test.run();
    }

    #[test]
    fn parsing_pyvenv_cfg_with_key_but_no_value_fails() {
        let system = TestSystem::default();
        let memory_fs = system.memory_file_system();
        let pyvenv_cfg_path = SystemPathBuf::from("/.venv/pyvenv.cfg");
        memory_fs
            .write_file_all(&pyvenv_cfg_path, "home =")
            .unwrap();
        let venv_result =
            PythonEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorKind::MalformedKeyValuePair { line_number }
            ))
            if path == pyvenv_cfg_path && Some(line_number) == NonZeroUsize::new(1)
        ));
    }

    #[test]
    fn parsing_pyvenv_cfg_with_value_but_no_key_fails() {
        let system = TestSystem::default();
        let memory_fs = system.memory_file_system();
        let pyvenv_cfg_path = SystemPathBuf::from("/.venv/pyvenv.cfg");
        memory_fs
            .write_file_all(&pyvenv_cfg_path, "= whatever")
            .unwrap();
        let venv_result =
            PythonEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorKind::MalformedKeyValuePair { line_number }
            ))
            if path == pyvenv_cfg_path && Some(line_number) == NonZeroUsize::new(1)
        ));
    }

    #[test]
    fn parsing_pyvenv_cfg_with_no_home_key_fails() {
        let system = TestSystem::default();
        let memory_fs = system.memory_file_system();
        let pyvenv_cfg_path = SystemPathBuf::from("/.venv/pyvenv.cfg");
        memory_fs.write_file_all(&pyvenv_cfg_path, "").unwrap();
        let venv_result =
            PythonEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorKind::NoHomeKey
            ))
            if path == pyvenv_cfg_path
        ));
    }

    #[test]
    fn parsing_pyvenv_cfg_with_invalid_home_key_fails() {
        let system = TestSystem::default();
        let memory_fs = system.memory_file_system();
        let pyvenv_cfg_path = SystemPathBuf::from("/.venv/pyvenv.cfg");
        memory_fs
            .write_file_all(&pyvenv_cfg_path, "home = foo")
            .unwrap();
        let venv_result =
            PythonEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorKind::InvalidHomeValue(_)
            ))
            if path == pyvenv_cfg_path
        ));
    }
}
