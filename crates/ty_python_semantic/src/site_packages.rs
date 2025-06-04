//! Utilities for finding the `site-packages` directory,
//! into which third-party packages are installed.
//!
//! The routines exposed by this module have different behaviour depending
//! on the platform of the *host machine*, which may be
//! different from the *target platform for type checking*. (A user
//! might be running ty on a Windows machine, but might
//! reasonably ask us to type-check code assuming that the code runs
//! on Linux.)

use std::fmt::Display;
use std::io;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::{fmt, sync::Arc};

use indexmap::IndexSet;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;
use ruff_python_trivia::Cursor;
use ruff_text_size::{TextLen, TextRange};

use crate::{PythonVersionFileSource, PythonVersionSource, PythonVersionWithSource};

type SitePackagesDiscoveryResult<T> = Result<T, SitePackagesDiscoveryError>;

/// An ordered, deduplicated set of `site-packages` search paths.
///
/// Most environments will only have one `site-packages` directory.
/// Some virtual environments created with `--system-site-packages`
/// will also have the system installation's `site-packages` packages
/// available, however. Ephemeral environments created with `uv` in
/// `uv run --with` invocations, meanwhile, "extend" a parent environment
/// (which could be another virtual environment or a system installation,
/// and which could itself have multiple `site-packages` directories).
///
/// We use an `IndexSet` here to guard against the (very remote)
/// possibility that an environment might somehow be marked as being
/// both a `--system-site-packages` virtual environment *and* an
/// ephemeral environment that extends the system environment. If this
/// were the case, the system environment's `site-packages` directory
/// *might* be added to the `SitePackagesPaths` twice, but we wouldn't
/// want duplicates to appear in this set.
#[derive(Debug, PartialEq, Eq, Default)]
pub(crate) struct SitePackagesPaths(IndexSet<SystemPathBuf>);

impl SitePackagesPaths {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    fn single(path: SystemPathBuf) -> Self {
        Self(IndexSet::from([path]))
    }

    fn insert(&mut self, path: SystemPathBuf) {
        self.0.insert(path);
    }

    fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

impl FromIterator<SystemPathBuf> for SitePackagesPaths {
    fn from_iter<T: IntoIterator<Item = SystemPathBuf>>(iter: T) -> Self {
        Self(IndexSet::from_iter(iter))
    }
}

impl IntoIterator for SitePackagesPaths {
    type Item = SystemPathBuf;
    type IntoIter = indexmap::set::IntoIter<SystemPathBuf>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl PartialEq<&[SystemPathBuf]> for SitePackagesPaths {
    fn eq(&self, other: &&[SystemPathBuf]) -> bool {
        self.0.as_slice() == *other
    }
}

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
        let path = SysPrefixPath::new(path.as_ref(), origin, system)?;

        // Attempt to inspect as a virtual environment first
        match VirtualEnvironment::new(path, system) {
            Ok(venv) => Ok(Self::Virtual(venv)),
            // If there's not a `pyvenv.cfg` marker, attempt to inspect as a system environment
            Err(SitePackagesDiscoveryError::NoPyvenvCfgFile(path, _))
                if !origin.must_be_virtual_env() =>
            {
                Ok(Self::System(SystemEnvironment::new(path)))
            }
            Err(err) => Err(err),
        }
    }

    /// Returns the `site-packages` directories for this Python environment,
    /// as well as the Python version that was used to create this environment
    /// (the latter will only be available for virtual environments that specify
    /// the metadata in their `pyvenv.cfg` files).
    ///
    /// See the documentation for [`site_packages_directory_from_sys_prefix`] for more details.
    pub(crate) fn into_settings(
        self,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<(SitePackagesPaths, Option<PythonVersionWithSource>)> {
        Ok(match self {
            Self::Virtual(venv) => (venv.site_packages_directories(system)?, venv.version),
            Self::System(env) => (env.site_packages_directories(system)?, None),
        })
    }

    fn site_packages_directories(
        &self,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<SitePackagesPaths> {
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
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub(crate) enum PythonImplementation {
    CPython,
    PyPy,
    GraalPy,
    /// Fallback when the value is missing or unrecognised.
    /// We treat it like CPython but keep the information for diagnostics.
    #[default]
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
    version: Option<PythonVersionWithSource>,
    implementation: PythonImplementation,

    /// If this virtual environment was created using uv,
    /// it may be an "ephemeral" virtual environment that dynamically adds the `site-packages`
    /// directories of its parent environment to `sys.path` at runtime.
    /// Newer versions of uv record the parent environment in the `pyvenv.cfg` file;
    /// we'll want to add the `site-packages` directories of the parent environment
    /// as search paths as well as the `site-packages` directories of this virtual environment.
    parent_environment: Option<Box<PythonEnvironment>>,
}

impl VirtualEnvironment {
    pub(crate) fn new(
        path: SysPrefixPath,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Self> {
        let pyvenv_cfg_path = path.join("pyvenv.cfg");
        tracing::debug!("Attempting to parse virtual environment metadata at '{pyvenv_cfg_path}'");

        let pyvenv_cfg = match system.read_to_string(&pyvenv_cfg_path) {
            Ok(pyvenv_cfg) => pyvenv_cfg,
            Err(err) => return Err(SitePackagesDiscoveryError::NoPyvenvCfgFile(path, err)),
        };

        let parsed_pyvenv_cfg =
            PyvenvCfgParser::new(&pyvenv_cfg)
                .parse()
                .map_err(|pyvenv_parse_error| {
                    SitePackagesDiscoveryError::PyvenvCfgParseError(
                        pyvenv_cfg_path.clone(),
                        pyvenv_parse_error,
                    )
                })?;

        let RawPyvenvCfg {
            include_system_site_packages,
            base_executable_home_path,
            version,
            implementation,
            created_with_uv,
            parent_environment,
        } = parsed_pyvenv_cfg;

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
                    pyvenv_cfg_path.clone(),
                    PyvenvCfgParseErrorKind::InvalidHomeValue(io_err),
                )
            })?;

        // Since the `extends-environment` key is nonstandard,
        // for now we only trust it if the virtual environment was created with `uv`.
        let parent_environment = if created_with_uv {
            parent_environment
                .and_then(|sys_prefix| {
                    PythonEnvironment::new(sys_prefix, SysPrefixPathOrigin::DerivedFromPyvenvCfg, system)
                    .inspect_err(|err| {
                        tracing::warn!(
                            "Failed to resolve the parent environment of this ephemeral uv virtual environment \
                            from the `extends-environment` value specified in the `pyvenv.cfg` file at {pyvenv_cfg_path}. \
                            Imports will not be resolved correctly if they refer to packages installed into the parent \
                            environment. Underlying error: {err}",
                        );
                    })
                    .ok()
                })
                .map(Box::new)
        } else {
            None
        };

        // but the `version`/`version_info` key is not read by the standard library,
        // and is provided under different keys depending on which virtual-environment creation tool
        // created the `pyvenv.cfg` file. Lenient parsing is appropriate here:
        // the file isn't really *invalid* if it doesn't have this key,
        // or if the value doesn't parse according to our expectations.
        let version = version.and_then(|(version_string, range)| {
            let mut version_info_parts = version_string.split('.');
            let (major, minor) = (version_info_parts.next()?, version_info_parts.next()?);
            let version = PythonVersion::try_from((major, minor)).ok()?;
            let source = PythonVersionSource::PyvenvCfgFile(PythonVersionFileSource::new(
                Arc::new(pyvenv_cfg_path),
                Some(range),
            ));
            Some(PythonVersionWithSource { version, source })
        });

        let metadata = Self {
            root_path: path,
            base_executable_home_path,
            include_system_site_packages,
            version,
            implementation,
            parent_environment,
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
    ) -> SitePackagesDiscoveryResult<SitePackagesPaths> {
        let VirtualEnvironment {
            root_path,
            base_executable_home_path,
            include_system_site_packages,
            implementation,
            version,
            parent_environment,
        } = self;

        let version = version.as_ref().map(|v| v.version);

        let mut site_packages_directories = SitePackagesPaths::single(
            site_packages_directory_from_sys_prefix(root_path, version, *implementation, system)?,
        );

        if let Some(parent_env_site_packages) = parent_environment.as_deref() {
            match parent_env_site_packages.site_packages_directories(system) {
                Ok(parent_environment_site_packages) => {
                    site_packages_directories.extend(parent_environment_site_packages);
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to resolve the site-packages directories of this ephemeral uv virtual environment's \
                        parent environment. Imports will not be resolved correctly if they refer to packages installed \
                        into the parent environment. Underlying error: {err}"
                    );
                }
            }
        }

        if *include_system_site_packages {
            let system_sys_prefix =
                SysPrefixPath::from_executable_home_path(base_executable_home_path);

            // If we fail to resolve the `sys.prefix` path from the base executable home path,
            // or if we fail to resolve the `site-packages` from the `sys.prefix` path,
            // we should probably print a warning but *not* abort type checking
            if let Some(sys_prefix_path) = system_sys_prefix {
                match site_packages_directory_from_sys_prefix(
                    &sys_prefix_path,
                    version,
                    *implementation,
                    system,
                ) {
                    Ok(site_packages_directory) => {
                        site_packages_directories.insert(site_packages_directory);
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

/// A parser for `pyvenv.cfg` files: metadata files for virtual environments.
///
/// Note that a `pyvenv.cfg` file *looks* like a `.ini` file, but actually isn't valid `.ini` syntax!
///
/// See also: <https://snarky.ca/how-virtual-environments-work/>
#[derive(Debug)]
struct PyvenvCfgParser<'s> {
    source: &'s str,
    cursor: Cursor<'s>,
    line_number: NonZeroUsize,
    data: RawPyvenvCfg<'s>,
}

impl<'s> PyvenvCfgParser<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            source,
            cursor: Cursor::new(source),
            line_number: NonZeroUsize::new(1).unwrap(),
            data: RawPyvenvCfg::default(),
        }
    }

    /// Parse the `pyvenv.cfg` file and return the parsed data.
    fn parse(mut self) -> Result<RawPyvenvCfg<'s>, PyvenvCfgParseErrorKind> {
        while !self.cursor.is_eof() {
            self.parse_line()?;
            self.line_number = self.line_number.checked_add(1).unwrap();
        }
        Ok(self.data)
    }

    /// Parse a single line of the `pyvenv.cfg` file and advance the cursor
    /// to the beginning of the next line.
    fn parse_line(&mut self) -> Result<(), PyvenvCfgParseErrorKind> {
        let PyvenvCfgParser {
            source,
            cursor,
            line_number,
            data,
        } = self;

        let line_number = *line_number;

        cursor.eat_while(|c| c.is_whitespace() && c != '\n');

        let key_start = cursor.offset();
        cursor.eat_while(|c| !matches!(c, '\n' | '='));
        let key_end = cursor.offset();

        if !cursor.eat_char('=') {
            // Skip over any lines that do not contain '=' characters, same as the CPython stdlib
            // <https://github.com/python/cpython/blob/e64395e8eb8d3a9e35e3e534e87d427ff27ab0a5/Lib/site.py#L625-L632>
            cursor.eat_char('\n');
            return Ok(());
        }

        let key = source[TextRange::new(key_start, key_end)].trim();

        cursor.eat_while(|c| c.is_whitespace() && c != '\n');
        let value_start = cursor.offset();
        cursor.eat_while(|c| c != '\n');
        let value = source[TextRange::new(value_start, cursor.offset())].trim();
        cursor.eat_char('\n');

        if value.is_empty() {
            return Err(PyvenvCfgParseErrorKind::MalformedKeyValuePair { line_number });
        }

        match key {
            "include-system-site-packages" => {
                data.include_system_site_packages = value.eq_ignore_ascii_case("true");
            }
            "home" => data.base_executable_home_path = Some(value),
            // `virtualenv` and `uv` call this key `version_info`,
            // but the stdlib venv module calls it `version`
            "version" | "version_info" => {
                let version_range = TextRange::at(value_start, value.text_len());
                data.version = Some((value, version_range));
            }
            "implementation" => {
                data.implementation = match value.to_ascii_lowercase().as_str() {
                    "cpython" => PythonImplementation::CPython,
                    "graalvm" => PythonImplementation::GraalPy,
                    "pypy" => PythonImplementation::PyPy,
                    _ => PythonImplementation::Unknown,
                };
            }
            "uv" => data.created_with_uv = true,
            "extends-environment" => data.parent_environment = Some(value),
            "" => {
                return Err(PyvenvCfgParseErrorKind::MalformedKeyValuePair { line_number });
            }
            _ => {}
        }

        Ok(())
    }
}

/// A `key:value` mapping derived from parsing a `pyvenv.cfg` file.
///
/// This data contained within is still mostly raw and unvalidated.
#[derive(Debug, Default)]
struct RawPyvenvCfg<'s> {
    include_system_site_packages: bool,
    base_executable_home_path: Option<&'s str>,
    version: Option<(&'s str, TextRange)>,
    implementation: PythonImplementation,
    created_with_uv: bool,
    parent_environment: Option<&'s str>,
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
    ) -> SitePackagesDiscoveryResult<SitePackagesPaths> {
        let SystemEnvironment { root_path } = self;

        let site_packages_directories =
            SitePackagesPaths::single(site_packages_directory_from_sys_prefix(
                root_path,
                None,
                PythonImplementation::Unknown,
                system,
            )?);

        tracing::debug!(
            "Resolved site-packages directories for this environment are: {site_packages_directories:?}"
        );
        Ok(site_packages_directories)
    }
}

/// Enumeration of ways in which `site-packages` discovery can fail.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SitePackagesDiscoveryError {
    /// `site-packages` discovery failed because the provided path couldn't be canonicalized.
    #[error("Invalid {1}: `{0}` could not be canonicalized")]
    CanonicalizationError(SystemPathBuf, SysPrefixPathOrigin, #[source] io::Error),

    /// `site-packages` discovery failed because the provided path doesn't appear to point to
    /// a Python executable or a `sys.prefix` directory.
    #[error(
        "Invalid {1}: `{0}` does not point to a {thing}",

        thing = if .1.must_point_directly_to_sys_prefix() {
            "directory on disk"
        } else {
            "Python executable or a directory on disk"
        }
    )]
    PathNotExecutableOrDirectory(SystemPathBuf, SysPrefixPathOrigin),

    /// `site-packages` discovery failed because the [`SysPrefixPathOrigin`] indicated that
    /// the provided path should point to the `sys.prefix` of a virtual environment,
    /// but there was no file at `<sys.prefix>/pyvenv.cfg`.
    #[error("{} points to a broken venv with no pyvenv.cfg file", .0.origin)]
    NoPyvenvCfgFile(SysPrefixPath, #[source] io::Error),

    /// `site-packages` discovery failed because the `pyvenv.cfg` file could not be parsed.
    #[error("Failed to parse the pyvenv.cfg file at {0} because {1}")]
    PyvenvCfgParseError(SystemPathBuf, PyvenvCfgParseErrorKind),

    /// `site-packages` discovery failed because we're on a Unix system,
    /// we weren't able to figure out from the `pyvenv.cfg` file exactly where `site-packages`
    /// would be relative to the `sys.prefix` path, and we tried to fallback to iterating
    /// through the `<sys.prefix>/lib` directory looking for a `site-packages` directory,
    /// but we came across some I/O error while trying to do so.
    #[error(
        "Failed to iterate over the contents of the `lib` directory of the Python installation at {1}"
    )]
    CouldNotReadLibDirectory(#[source] io::Error, SysPrefixPath),

    /// We looked everywhere we could think of for the `site-packages` directory,
    /// but none could be found despite our best endeavours.
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
                let unvalidated_path = unvalidated_path.to_path_buf();
                if io_err.kind() == io::ErrorKind::NotFound {
                    SitePackagesDiscoveryError::PathNotExecutableOrDirectory(
                        unvalidated_path,
                        origin,
                    )
                } else {
                    SitePackagesDiscoveryError::CanonicalizationError(
                        unvalidated_path,
                        origin,
                        io_err,
                    )
                }
            })?;

        if origin.must_point_directly_to_sys_prefix() {
            return system
                .is_directory(&canonicalized)
                .then_some(Self {
                    inner: canonicalized,
                    origin,
                })
                .ok_or_else(|| {
                    SitePackagesDiscoveryError::PathNotExecutableOrDirectory(
                        unvalidated_path.to_path_buf(),
                        origin,
                    )
                });
        }

        let sys_prefix = if system.is_file(&canonicalized)
            && canonicalized
                .file_name()
                .is_some_and(|name| name.starts_with("python"))
        {
            // It looks like they passed us a path to a Python executable, e.g. `.venv/bin/python3`.
            // Try to figure out the `sys.prefix` value from the Python executable.
            let sys_prefix = if cfg!(windows) {
                // On Windows, the relative path to the Python executable from `sys.prefix`
                // is different depending on whether it's a virtual environment or a system installation.
                // System installations have their executable at `<sys.prefix>/python.exe`,
                // whereas virtual environments have their executable at `<sys.prefix>/Scripts/python.exe`.
                canonicalized.parent().and_then(|parent| {
                    if parent.file_name() == Some("Scripts") {
                        parent.parent()
                    } else {
                        Some(parent)
                    }
                })
            } else {
                // On Unix, `sys.prefix` is always the grandparent directory of the Python executable,
                // regardless of whether it's a virtual environment or a system installation.
                canonicalized.ancestors().nth(2)
            };
            sys_prefix.map(SystemPath::to_path_buf).ok_or_else(|| {
                SitePackagesDiscoveryError::PathNotExecutableOrDirectory(
                    unvalidated_path.to_path_buf(),
                    origin,
                )
            })?
        } else if system.is_directory(&canonicalized) {
            canonicalized
        } else {
            return Err(SitePackagesDiscoveryError::PathNotExecutableOrDirectory(
                unvalidated_path.to_path_buf(),
                origin,
            ));
        };

        Ok(Self {
            inner: sys_prefix,
            origin,
        })
    }

    fn from_executable_home_path(path: &PythonHomePath) -> Option<Self> {
        // No need to check whether `path.parent()` is a directory:
        // the parent of a canonicalised path that is known to exist
        // is guaranteed to be a directory.
        if cfg!(target_os = "windows") {
            Some(Self {
                inner: path.to_path_buf(),
                origin: SysPrefixPathOrigin::DerivedFromPyvenvCfg,
            })
        } else {
            path.parent().map(|path| Self {
                inner: path.to_path_buf(),
                origin: SysPrefixPathOrigin::DerivedFromPyvenvCfg,
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

/// Enumeration of sources a `sys.prefix` path can come from.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SysPrefixPathOrigin {
    /// The `sys.prefix` path came from a `--python` CLI flag
    PythonCliFlag,
    /// The `sys.prefix` path came from the `VIRTUAL_ENV` environment variable
    VirtualEnvVar,
    /// The `sys.prefix` path came from the `CONDA_PREFIX` environment variable
    CondaPrefixVar,
    /// The `sys.prefix` path was derived from a value in a `pyvenv.cfg` file:
    /// either the value associated with the `home` key
    /// or the value associated with the `extends-environment` key
    DerivedFromPyvenvCfg,
    /// A `.venv` directory was found in the current working directory,
    /// and the `sys.prefix` path is the path to that virtual environment.
    LocalVenv,
}

impl SysPrefixPathOrigin {
    /// Whether the given `sys.prefix` path must be a virtual environment (rather than a system
    /// Python environment).
    pub(crate) const fn must_be_virtual_env(self) -> bool {
        match self {
            Self::LocalVenv | Self::VirtualEnvVar => true,
            Self::PythonCliFlag | Self::DerivedFromPyvenvCfg | Self::CondaPrefixVar => false,
        }
    }

    /// Whether paths with this origin always point directly to the `sys.prefix` directory.
    ///
    /// Some variants can point either directly to `sys.prefix` or to a Python executable inside
    /// the `sys.prefix` directory, e.g. the `--python` CLI flag.
    pub(crate) const fn must_point_directly_to_sys_prefix(self) -> bool {
        match self {
            Self::PythonCliFlag => false,
            Self::VirtualEnvVar
            | Self::CondaPrefixVar
            | Self::DerivedFromPyvenvCfg
            | Self::LocalVenv => true,
        }
    }
}

impl Display for SysPrefixPathOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::PythonCliFlag => f.write_str("`--python` argument"),
            Self::VirtualEnvVar => f.write_str("`VIRTUAL_ENV` environment variable"),
            Self::CondaPrefixVar => f.write_str("`CONDA_PREFIX` environment variable"),
            Self::DerivedFromPyvenvCfg => f.write_str("derived `sys.prefix` path"),
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

    #[derive(Default)]
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

        #[track_caller]
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
                    venv.version.as_ref().map(|v| v.version),
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

            let expected_system_site_packages = self.expected_system_site_packages();

            if self_venv.system_site_packages {
                assert_eq!(
                    site_packages_directories,
                    [expected_venv_site_packages, expected_system_site_packages].as_slice()
                );
            } else {
                assert_eq!(
                    &site_packages_directories.into_iter().next().unwrap(),
                    &expected_venv_site_packages
                );
            }
        }

        #[track_caller]
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
            let expected_site_packages = self.expected_system_site_packages();
            assert_eq!(
                site_packages_directories,
                std::slice::from_ref(&expected_site_packages)
            );
        }

        fn expected_system_site_packages(&self) -> SystemPathBuf {
            let minor_version = self.minor_version;
            if cfg!(target_os = "windows") {
                SystemPathBuf::from(&*format!(r"\Python3.{minor_version}\Lib\site-packages"))
            } else if self.free_threaded {
                SystemPathBuf::from(&*format!(
                    "/Python3.{minor_version}/lib/python3.{minor_version}t/site-packages"
                ))
            } else {
                SystemPathBuf::from(&*format!(
                    "/Python3.{minor_version}/lib/python3.{minor_version}/site-packages"
                ))
            }
        }
    }

    #[test]
    fn can_find_site_packages_directory_no_virtual_env() {
        // Shouldn't be converted to an mdtest because mdtest automatically creates a
        // pyvenv.cfg file for you if it sees you creating a `site-packages` directory.
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
        // Shouldn't be converted to an mdtest because mdtest automatically creates a
        // pyvenv.cfg file for you if it sees you creating a `site-packages` directory.
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
    fn can_find_site_packages_directory_venv_style_version_field_in_pyvenv_cfg() {
        // Shouldn't be converted to an mdtest because we want to assert
        // that we parsed the `version` field correctly in `test.run()`.
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                pyvenv_cfg_version_field: Some("version = 3.12"),
                ..VirtualEnvironmentTestCase::default()
            }),
        };
        test.run();
    }

    #[test]
    fn can_find_site_packages_directory_uv_style_version_field_in_pyvenv_cfg() {
        // Shouldn't be converted to an mdtest because we want to assert
        // that we parsed the `version` field correctly in `test.run()`.
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                pyvenv_cfg_version_field: Some("version_info = 3.12"),
                ..VirtualEnvironmentTestCase::default()
            }),
        };
        test.run();
    }

    #[test]
    fn can_find_site_packages_directory_virtualenv_style_version_field_in_pyvenv_cfg() {
        // Shouldn't be converted to an mdtest because we want to assert
        // that we parsed the `version` field correctly in `test.run()`.
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                pyvenv_cfg_version_field: Some("version_info = 3.12.0rc2"),
                ..VirtualEnvironmentTestCase::default()
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
                pyvenv_cfg_version_field: Some("version_info = 3.13"),
                ..VirtualEnvironmentTestCase::default()
            }),
        };
        test.run();
    }

    #[test]
    fn finds_system_site_packages() {
        // Can't be converted to an mdtest because the system installation's `sys.prefix`
        // path is at a different location relative to the `pyvenv.cfg` file's `home` value
        // on Windows.
        let test = PythonEnvironmentTestCase {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            origin: SysPrefixPathOrigin::VirtualEnvVar,
            virtual_env: Some(VirtualEnvironmentTestCase {
                system_site_packages: true,
                pyvenv_cfg_version_field: Some("version_info = 3.13"),
                ..VirtualEnvironmentTestCase::default()
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
                implementation_field: Some("implementation = PyPy"),
                ..VirtualEnvironmentTestCase::default()
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
                implementation_field: Some("implementation = CPython"),
                ..VirtualEnvironmentTestCase::default()
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
                implementation_field: Some("implementation = GraalVM"),
                ..VirtualEnvironmentTestCase::default()
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
            virtual_env: Some(VirtualEnvironmentTestCase::default()),
        };
        let venv = test.run().expect_venv();
        assert_eq!(venv.implementation, PythonImplementation::Unknown);
    }

    #[test]
    fn reject_env_that_does_not_exist() {
        let system = TestSystem::default();
        assert!(matches!(
            PythonEnvironment::new("/env", SysPrefixPathOrigin::PythonCliFlag, &system),
            Err(SitePackagesDiscoveryError::PathNotExecutableOrDirectory(..))
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
            Err(SitePackagesDiscoveryError::PathNotExecutableOrDirectory(..))
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

    #[test]
    fn pyvenv_cfg_with_carriage_return_line_endings_parses() {
        let pyvenv_cfg = "home = /somewhere/python\r\nversion_info = 3.13\r\nimplementation = PyPy";
        let parsed = PyvenvCfgParser::new(pyvenv_cfg).parse().unwrap();
        assert_eq!(parsed.base_executable_home_path, Some("/somewhere/python"));
        let version = parsed.version.unwrap();
        assert_eq!(version.0, "3.13");
        assert_eq!(&pyvenv_cfg[version.1], version.0);
        assert_eq!(parsed.implementation, PythonImplementation::PyPy);
    }

    #[test]
    fn pyvenv_cfg_with_strange_whitespace_parses() {
        let pyvenv_cfg = "  home= /a path with whitespace/python\t   \t  \nversion_info =    3.13 \n\n\n\nimplementation    =PyPy";
        let parsed = PyvenvCfgParser::new(pyvenv_cfg).parse().unwrap();
        assert_eq!(
            parsed.base_executable_home_path,
            Some("/a path with whitespace/python")
        );
        let version = parsed.version.unwrap();
        assert_eq!(version.0, "3.13");
        assert_eq!(&pyvenv_cfg[version.1], version.0);
        assert_eq!(parsed.implementation, PythonImplementation::PyPy);
    }
}
