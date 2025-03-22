//! Utilities for finding the `site-packages` directory,
//! into which third-party packages are installed.
//!
//! The routines exposed by this module have different behaviour depending
//! on the platform of the *host machine*, which may be
//! different from the *target platform for type checking*. (A user
//! might be running red-knot on a Windows machine, but might
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

/// Abstraction for a Python virtual environment.
///
/// Most of this information is derived from the virtual environment's `pyvenv.cfg` file.
/// The format of this file is not defined anywhere, and exactly which keys are present
/// depends on the tool that was used to create the virtual environment.
#[derive(Debug)]
pub(crate) struct VirtualEnvironment {
    venv_path: SysPrefixPath,
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
}

impl VirtualEnvironment {
    pub(crate) fn new(
        path: impl AsRef<SystemPath>,
        origin: SysPrefixPathOrigin,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Self> {
        Self::new_impl(path.as_ref(), origin, system)
    }

    fn new_impl(
        path: &SystemPath,
        origin: SysPrefixPathOrigin,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Self> {
        fn pyvenv_cfg_line_number(index: usize) -> NonZeroUsize {
            index.checked_add(1).and_then(NonZeroUsize::new).unwrap()
        }

        let venv_path = SysPrefixPath::new(path, origin, system)?;
        let pyvenv_cfg_path = venv_path.join("pyvenv.cfg");
        tracing::debug!("Attempting to parse virtual environment metadata at '{pyvenv_cfg_path}'");

        let pyvenv_cfg = system
            .read_to_string(&pyvenv_cfg_path)
            .map_err(|io_err| SitePackagesDiscoveryError::NoPyvenvCfgFile(origin, io_err))?;

        let mut include_system_site_packages = false;
        let mut base_executable_home_path = None;
        let mut version_info_string = None;

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

                if value.contains('=') {
                    return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                        pyvenv_cfg_path,
                        PyvenvCfgParseErrorKind::TooManyEquals {
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
            venv_path,
            base_executable_home_path,
            include_system_site_packages,
            version,
        };

        tracing::trace!("Resolved metadata for virtual environment: {metadata:?}");
        Ok(metadata)
    }

    /// Return a list of `site-packages` directories that are available from this virtual environment
    ///
    /// See the documentation for `site_packages_dir_from_sys_prefix` for more details.
    pub(crate) fn site_packages_directories(
        &self,
        system: &dyn System,
    ) -> SitePackagesDiscoveryResult<Vec<SystemPathBuf>> {
        let VirtualEnvironment {
            venv_path,
            base_executable_home_path,
            include_system_site_packages,
            version,
        } = self;

        let mut site_packages_directories = vec![site_packages_directory_from_sys_prefix(
            venv_path, *version, system,
        )?];

        if *include_system_site_packages {
            let system_sys_prefix =
                SysPrefixPath::from_executable_home_path(base_executable_home_path);

            // If we fail to resolve the `sys.prefix` path from the base executable home path,
            // or if we fail to resolve the `site-packages` from the `sys.prefix` path,
            // we should probably print a warning but *not* abort type checking
            if let Some(sys_prefix_path) = system_sys_prefix {
                match site_packages_directory_from_sys_prefix(&sys_prefix_path, *version, system) {
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
                    venv_path.join("pyvenv.cfg")
                );
            }
        }

        tracing::debug!("Resolved site-packages directories for this virtual environment are: {site_packages_directories:?}");
        Ok(site_packages_directories)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SitePackagesDiscoveryError {
    #[error("Invalid {1}: `{0}` could not be canonicalized")]
    VenvDirCanonicalizationError(SystemPathBuf, SysPrefixPathOrigin, #[source] io::Error),
    #[error("Invalid {1}: `{0}` does not point to a directory on disk")]
    VenvDirIsNotADirectory(SystemPathBuf, SysPrefixPathOrigin),
    #[error("{0} points to a broken venv with no pyvenv.cfg file")]
    NoPyvenvCfgFile(SysPrefixPathOrigin, #[source] io::Error),
    #[error("Failed to parse the pyvenv.cfg file at {0} because {1}")]
    PyvenvCfgParseError(SystemPathBuf, PyvenvCfgParseErrorKind),
    #[error("Failed to search the `lib` directory of the Python installation at {1} for `site-packages`")]
    CouldNotReadLibDirectory(#[source] io::Error, SysPrefixPath),
    #[error("Could not find the `site-packages` directory for the Python installation at {0}")]
    NoSitePackagesDirFound(SysPrefixPath),
}

/// The various ways in which parsing a `pyvenv.cfg` file could fail
#[derive(Debug)]
pub(crate) enum PyvenvCfgParseErrorKind {
    TooManyEquals { line_number: NonZeroUsize },
    MalformedKeyValuePair { line_number: NonZeroUsize },
    NoHomeKey,
    InvalidHomeValue(io::Error),
}

impl fmt::Display for PyvenvCfgParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyEquals { line_number } => {
                write!(f, "line {line_number} has too many '=' characters")
            }
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
    if let Some(version) = python_version {
        let expected_path = sys_prefix_path.join(format!("lib/python{version}/site-packages"));
        if system.is_directory(&expected_path) {
            return Ok(expected_path);
        }
        if version.free_threaded_build_available() {
            // Nearly the same as `expected_path`, but with an additional `t` after {version}:
            let alternative_path =
                sys_prefix_path.join(format!("lib/python{version}t/site-packages"));
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

        if !name.starts_with("python3.") {
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
                SitePackagesDiscoveryError::VenvDirCanonicalizationError(
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
                SitePackagesDiscoveryError::VenvDirIsNotADirectory(
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
    Derived,
    LocalVenv,
}

impl Display for SysPrefixPathOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::PythonCliFlag => f.write_str("`--python` argument"),
            Self::VirtualEnvVar => f.write_str("`VIRTUAL_ENV` environment variable"),
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
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "not a directory"))
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

    struct VirtualEnvironmentTester {
        system: TestSystem,
        minor_version: u8,
        free_threaded: bool,
        system_site_packages: bool,
        pyvenv_cfg_version_field: Option<&'static str>,
    }

    impl VirtualEnvironmentTester {
        /// Builds a mock virtual environment, and returns the path to the venv
        fn build_mock_venv(&self) -> SystemPathBuf {
            let VirtualEnvironmentTester {
                system,
                minor_version,
                system_site_packages,
                free_threaded,
                pyvenv_cfg_version_field,
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
            // Deliberately using weird casing here to test that our pyvenv.cfg parsing is case-insensitive:
            if *system_site_packages {
                pyvenv_cfg_contents.push_str("include-system-site-packages = TRuE\n");
            }
            memory_fs
                .write_file_all(pyvenv_cfg_path, &pyvenv_cfg_contents)
                .unwrap();

            venv_sys_prefix
        }

        fn test(self) {
            let venv_path = self.build_mock_venv();
            let venv = VirtualEnvironment::new(
                venv_path.clone(),
                SysPrefixPathOrigin::VirtualEnvVar,
                &self.system,
            )
            .unwrap();

            assert_eq!(
                venv.venv_path,
                SysPrefixPath {
                    inner: self.system.canonicalize_path(&venv_path).unwrap(),
                    origin: SysPrefixPathOrigin::VirtualEnvVar,
                }
            );
            assert_eq!(venv.include_system_site_packages, self.system_site_packages);

            if self.pyvenv_cfg_version_field.is_some() {
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

            if self.system_site_packages {
                assert_eq!(
                    &site_packages_directories,
                    &[expected_venv_site_packages, expected_system_site_packages]
                );
            } else {
                assert_eq!(&site_packages_directories, &[expected_venv_site_packages]);
            }
        }
    }

    #[test]
    fn can_find_site_packages_directory_no_version_field_in_pyvenv_cfg() {
        let tester = VirtualEnvironmentTester {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            system_site_packages: false,
            pyvenv_cfg_version_field: None,
        };
        tester.test();
    }

    #[test]
    fn can_find_site_packages_directory_venv_style_version_field_in_pyvenv_cfg() {
        let tester = VirtualEnvironmentTester {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            system_site_packages: false,
            pyvenv_cfg_version_field: Some("version = 3.12"),
        };
        tester.test();
    }

    #[test]
    fn can_find_site_packages_directory_uv_style_version_field_in_pyvenv_cfg() {
        let tester = VirtualEnvironmentTester {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            system_site_packages: false,
            pyvenv_cfg_version_field: Some("version_info = 3.12"),
        };
        tester.test();
    }

    #[test]
    fn can_find_site_packages_directory_virtualenv_style_version_field_in_pyvenv_cfg() {
        let tester = VirtualEnvironmentTester {
            system: TestSystem::default(),
            minor_version: 12,
            free_threaded: false,
            system_site_packages: false,
            pyvenv_cfg_version_field: Some("version_info = 3.12.0rc2"),
        };
        tester.test();
    }

    #[test]
    fn can_find_site_packages_directory_freethreaded_build() {
        let tester = VirtualEnvironmentTester {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            system_site_packages: false,
            pyvenv_cfg_version_field: Some("version_info = 3.13"),
        };
        tester.test();
    }

    #[test]
    fn finds_system_site_packages() {
        let tester = VirtualEnvironmentTester {
            system: TestSystem::default(),
            minor_version: 13,
            free_threaded: true,
            system_site_packages: true,
            pyvenv_cfg_version_field: Some("version_info = 3.13"),
        };
        tester.test();
    }

    #[test]
    fn reject_venv_that_does_not_exist() {
        let system = TestSystem::default();
        assert!(matches!(
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system),
            Err(SitePackagesDiscoveryError::VenvDirCanonicalizationError(..))
        ));
    }

    #[test]
    fn reject_venv_that_is_not_a_directory() {
        let system = TestSystem::default();
        system
            .memory_file_system()
            .write_file_all("/.venv", "")
            .unwrap();
        assert!(matches!(
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system),
            Err(SitePackagesDiscoveryError::VenvDirIsNotADirectory(..))
        ));
    }

    #[test]
    fn reject_venv_with_no_pyvenv_cfg_file() {
        let system = TestSystem::default();
        system
            .memory_file_system()
            .create_directory_all("/.venv")
            .unwrap();
        assert!(matches!(
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system),
            Err(SitePackagesDiscoveryError::NoPyvenvCfgFile(
                SysPrefixPathOrigin::VirtualEnvVar,
                _
            ))
        ));
    }

    #[test]
    fn parsing_pyvenv_cfg_with_too_many_equals() {
        let system = TestSystem::default();
        let memory_fs = system.memory_file_system();
        let pyvenv_cfg_path = SystemPathBuf::from("/.venv/pyvenv.cfg");
        memory_fs
            .write_file_all(&pyvenv_cfg_path, "home = bar = /.venv/bin")
            .unwrap();
        let venv_result =
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorKind::TooManyEquals { line_number }
            ))
            if path == pyvenv_cfg_path && Some(line_number) == NonZeroUsize::new(1)
        ));
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
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
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
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
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
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
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
            VirtualEnvironment::new("/.venv", SysPrefixPathOrigin::VirtualEnvVar, &system);
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
