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
use std::io;
use std::num::NonZeroUsize;
use std::ops::Deref;

use ruff_db::system::{System, SystemPath, SystemPathBuf};

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
/// standard libary's `site.py` module.
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
    fn new(path: impl AsRef<SystemPath>, system: &dyn System) -> Option<Self> {
        let path = path.as_ref();
        let canonicalized = system
            .canonicalize_path(path)
            .unwrap_or_else(|_| path.to_path_buf());
        system
            .is_directory(&canonicalized)
            .then_some(Self(canonicalized))
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
        write!(f, "`home` location {}", self.0)
    }
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
#[derive(Debug, PartialEq, Eq)]
struct SysPrefixPath(SystemPathBuf);

impl SysPrefixPath {
    fn new(unvalidated_path: impl AsRef<SystemPath>, system: &dyn System) -> Option<Self> {
        let unvalidated_path = unvalidated_path.as_ref();
        let canonicalized = system
            .canonicalize_path(unvalidated_path)
            .unwrap_or_else(|_| unvalidated_path.to_path_buf());
        system
            .is_directory(&canonicalized)
            .then_some(Self(canonicalized))
    }

    fn from_executable_home_path(path: &PythonHomePath, system: &dyn System) -> Option<Self> {
        let candidate = path.parent()?;
        system
            .is_directory(candidate)
            .then(|| Self(candidate.to_path_buf()))
    }
}

impl Deref for SysPrefixPath {
    type Target = SystemPath;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for SysPrefixPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`sys.prefix` path {}", self.0)
    }
}

/// E.g. `12` for Python 3.12.4
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PythonMinorVersion(u8);

impl PythonMinorVersion {
    fn from_version_string(version_string: &str) -> Option<Self> {
        version_string.parse().ok().map(Self)
    }

    const fn free_threaded_build_available(self) -> bool {
        self.0 >= 13
    }
}

impl Deref for PythonMinorVersion {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for PythonMinorVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq<u8> for PythonMinorVersion {
    fn eq(&self, other: &u8) -> bool {
        &self.0 == other
    }
}

/// Attempt to retrieve the `site-packages` directory
/// associated with a given Python installation.
fn site_packages_dir_from_sys_prefix(
    sys_prefix_path: &SysPrefixPath,
    venv_minor_version: Option<PythonMinorVersion>,
    system: &dyn System,
) -> Result<SystemPathBuf, SitePackagesDiscoveryError> {
    tracing::debug!("Searching for site-packages directory in {sys_prefix_path:?}");

    if cfg!(target_os = "windows") {
        let site_packages = sys_prefix_path.join("Lib/site-packages");
        return system
            .is_directory(&site_packages)
            .then_some(site_packages)
            .ok_or(SitePackagesDiscoveryError::NoSitePackagesDirFound);
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

    // If we were able to figure out from the `pyvenv.cfg` file what Python version this venv is for,
    // we should be able to avoid iterating through all items in the `lib/` directory:
    if let Some(minor_version) = venv_minor_version {
        let expected_path =
            sys_prefix_path.join(format!("lib/python3.{minor_version}/site-packages"));
        if system.is_directory(&expected_path) {
            return Ok(expected_path);
        }
        if minor_version.free_threaded_build_available() {
            let alternative_path =
                sys_prefix_path.join(format!("lib/python3.{minor_version}t/site-packages"));
            if system.is_directory(&alternative_path) {
                return Ok(alternative_path);
            }
        }
    }

    // Either we couldn't figure out the minor version from the `pyvenv.cfg` file,
    // or we couldn't find a `site-packages` folder at the expected location given
    // the parsed minor version
    //
    // Note: the `python3.x` part of the `site-packages` path can't be computed from
    // the `--target-version` the user has passed, as they might be running Python 3.12 locally
    // even if they've requested that we type check their code "as if" they're running 3.8.
    for entry_result in system
        .read_directory(&sys_prefix_path.join("lib"))
        .map_err(SitePackagesDiscoveryError::CouldNotReadLibDirectory)?
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
    Err(SitePackagesDiscoveryError::NoSitePackagesDirFound)
}

/// The various ways in which parsing a `pyvenv.cfg` file could fail
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyvenvCfgParseErrorReason {
    TooManyEquals { line_number: NonZeroUsize },
    NoHomeKey,
    HomeValueIsNotADirectory,
}

impl fmt::Display for PyvenvCfgParseErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyEquals { line_number } => {
                write!(f, "line {line_number} has too many '=' characters")
            }
            Self::NoHomeKey => f.write_str("the file does not have a `home` key"),
            Self::HomeValueIsNotADirectory => {
                f.write_str("the value for the `home` key does not point to a directory on disk")
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SitePackagesDiscoveryError {
    #[error("Invalid --venv-path argument: {0} does not point to a directory on disk")]
    VenvDirIsNotADirectory(SystemPathBuf),
    #[error("--venv-path points to a broken venv with no pyvenv.cfg file")]
    NoPyvenvCfgFile(#[source] io::Error),
    #[error("Failed to parse the pyvenv.cfg file at {0} because {1}")]
    PyvenvCfgParseError(SystemPathBuf, PyvenvCfgParseErrorReason),
    #[error("Failed to search the virtual environment directory for `site-packages`")]
    CouldNotReadLibDirectory(#[source] io::Error),
    #[error("Could not find the `site-packages` directory in the virtual environment")]
    NoSitePackagesDirFound,
}

/// Abstraction for a Python virtual environment.
///
/// Most of this information is derived from the virtual environment's `pyvenv.cfg` file.
/// The format of this file is not defined anyway, and exactly which keys are present
/// depends on the tool that was used to create the virtual environment.
#[derive(Debug)]
pub struct VirtualEnvironment {
    venv_path: SysPrefixPath,
    base_executable_home_path: PythonHomePath,
    include_system_site_packages: bool,

    /// The minor version of the Python executable that was used to create this virtual environment.
    ///
    /// The Python version is encoded under different keys and in different formats
    /// by different virtual-environment creation tools,
    /// and the key is never read by the standard-library `site.py` module,
    /// so it's possible that we might not be able to find this information
    /// in an acceptable format under any of the keys we expect.
    /// This field will be `None` if so.
    minor_version: Option<PythonMinorVersion>,
}

impl VirtualEnvironment {
    pub fn new(
        path: impl Into<SystemPathBuf>,
        system: &dyn System,
    ) -> Result<Self, SitePackagesDiscoveryError> {
        let path = path.into();
        let Some(venv_path) = SysPrefixPath::new(&path, system) else {
            return Err(SitePackagesDiscoveryError::VenvDirIsNotADirectory(path));
        };

        let pyvenv_cfg_path = venv_path.join("pyvenv.cfg");
        tracing::debug!("Attempting to parse virtual environment metadata at {pyvenv_cfg_path}");

        let pyvenv_cfg = system
            .read_to_string(&pyvenv_cfg_path)
            .map_err(SitePackagesDiscoveryError::NoPyvenvCfgFile)?;

        let mut include_system_site_packages = false;
        let mut base_executable_home_path = None;
        let mut version_info_string = None;

        // A `pyvenv.cfg` file *looks* like a `.ini` file, but actually isn't valid `.ini` syntax!
        // The Python standard-library's `site` module parses these files by splitting each line on
        // '=' signs, so that's what we should do as well.
        //
        // See also: https://snarky.ca/how-virtual-environments-work/
        for (index, line) in pyvenv_cfg.lines().enumerate() {
            let mut line_split = line.split('=');
            if let Some(key) = line_split.next() {
                let (Some(value), None) = (line_split.next(), line_split.next()) else {
                    let line_number = index.checked_add(1).and_then(NonZeroUsize::new).unwrap();
                    return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                        pyvenv_cfg_path,
                        PyvenvCfgParseErrorReason::TooManyEquals { line_number },
                    ));
                };
                let key = key.trim();
                let value = value.trim();
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
                PyvenvCfgParseErrorReason::NoHomeKey,
            ));
        };
        let Some(base_executable_home_path) =
            PythonHomePath::new(base_executable_home_path, system)
        else {
            return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                pyvenv_cfg_path,
                PyvenvCfgParseErrorReason::HomeValueIsNotADirectory,
            ));
        };

        // but the `version`/`version_info` key is not read by the standard library,
        // and is provided under different keys depending on which virtual-environment creation tool
        // created the `pyvenv.cfg` file. Lenient parsing is appropriate here:
        // the file isn't really *invalid* if it doesn't have this key,
        // or if the value doesn't parse according to our expectations.
        let minor_version = version_info_string.and_then(|version_string| {
            let mut version_info_parts = version_string.split('.');
            if version_info_parts.next()? != "3" {
                return None;
            }
            let minor_version_string = version_info_parts.next()?;
            PythonMinorVersion::from_version_string(minor_version_string)
        });

        let metadata = Self {
            venv_path,
            base_executable_home_path,
            include_system_site_packages,
            minor_version,
        };

        tracing::trace!("Resolved metadata for virtual environment: {metadata:?}");
        Ok(metadata)
    }

    /// Return a list of `site-packages` directories that are available from this virtual environment
    ///
    /// See the documentation for `site_packages_dir_from_sys_prefix` for more details.
    pub fn site_packages_dirs(
        &self,
        system: &dyn System,
    ) -> Result<Vec<SystemPathBuf>, SitePackagesDiscoveryError> {
        let VirtualEnvironment {
            venv_path,
            base_executable_home_path,
            include_system_site_packages,
            minor_version,
        } = self;

        let minor_version = minor_version.as_ref().copied();

        let mut site_packages_dirs = vec![site_packages_dir_from_sys_prefix(
            venv_path,
            minor_version,
            system,
        )?];

        if *include_system_site_packages {
            let system_sys_prefix =
                SysPrefixPath::from_executable_home_path(base_executable_home_path, system);

            // If we fail to resolve the `sys.prefix` path from the base executable home path,
            // or if we fail to resolve the `site-packages` from the `sys.prefix` path,
            // we should probably print a warning but *not* abort type checking
            if let Some(sys_prefix_path) = system_sys_prefix {
                if let Ok(site_packages_dir) =
                    site_packages_dir_from_sys_prefix(&sys_prefix_path, minor_version, system)
                {
                    site_packages_dirs.push(site_packages_dir);
                }
            }
        }

        tracing::debug!("Resolved site-packages directories for this virtual environment are: {site_packages_dirs:?}");
        Ok(site_packages_dirs)
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::system::{OsSystem, TestSystem};

    use super::*;

    fn check_venv_basics_at_path(venv_name: impl AsRef<SystemPath>) {
        let path_to_venv = SystemPath::new("resources/test").join(venv_name);
        let system = OsSystem::default();

        // if this doesn't hold true, the premise of the test is incorrect.
        assert!(system.is_directory(&path_to_venv));

        let virtual_environment = VirtualEnvironment::new(path_to_venv.clone(), &system).unwrap();
        assert_eq!(
            virtual_environment.venv_path,
            SysPrefixPath(system.canonicalize_path(&path_to_venv).unwrap())
        );
        assert!(!virtual_environment.include_system_site_packages);
        assert!(virtual_environment
            .minor_version
            .is_some_and(|minor_ver| minor_ver == 12));

        assert!(!virtual_environment
            .base_executable_home_path
            .as_str()
            .is_empty());

        let site_packages_dirs = virtual_environment.site_packages_dirs(&system).unwrap();
        assert_eq!(site_packages_dirs.len(), 1);
    }

    // Windows venvs have different layouts, and we only have Unix venvs committed for now.
    // These tests are skipped on Windows until we commit Windows venvs.

    #[test]
    #[cfg_attr(target_os = "windows", ignore = "Windows has a different venv layout")]
    fn can_find_site_packages_dir_in_uv_venv() {
        check_venv_basics_at_path("unix-uv-venv");
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore = "Windows has a different venv layout")]
    fn can_find_site_packages_dir_in_stdlib_venv() {
        check_venv_basics_at_path("unix-stdlib-venv");
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore = "Windows has a different venv layout")]
    fn can_find_site_packages_dir_in_virtualenv_venv() {
        check_venv_basics_at_path("unix-virtualenv-venv");
    }

    #[test]
    fn reject_venv_that_does_not_exist() {
        let system = TestSystem::default();
        assert!(matches!(
            VirtualEnvironment::new("/.venv", &system),
            Err(SitePackagesDiscoveryError::VenvDirIsNotADirectory(_))
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
            VirtualEnvironment::new("/.venv", &system),
            Err(SitePackagesDiscoveryError::NoPyvenvCfgFile(_))
        ));
    }

    #[test]
    fn parsing_pyvenv_cfg_with_invalid_syntax_fails() {
        let system = TestSystem::default();
        let memory_fs = system.memory_file_system();
        let pyvenv_cfg_path = SystemPathBuf::from("/.venv/pyvenv.cfg");
        memory_fs
            .write_file(&pyvenv_cfg_path, "home = bar = /.venv/bin")
            .unwrap();
        let venv_result = VirtualEnvironment::new("/.venv", &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorReason::TooManyEquals { line_number }
            ))
            if path == pyvenv_cfg_path && Some(line_number) == NonZeroUsize::new(1)
        ));
    }

    #[test]
    fn parsing_pyvenv_cfg_with_no_home_key_fails() {
        let system = TestSystem::default();
        let memory_fs = system.memory_file_system();
        let pyvenv_cfg_path = SystemPathBuf::from("/.venv/pyvenv.cfg");
        memory_fs.write_file(&pyvenv_cfg_path, "").unwrap();
        let venv_result = VirtualEnvironment::new("/.venv", &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorReason::NoHomeKey
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
            .write_file(&pyvenv_cfg_path, "home = foo")
            .unwrap();
        let venv_result = VirtualEnvironment::new("/.venv", &system);
        assert!(matches!(
            venv_result,
            Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                path,
                PyvenvCfgParseErrorReason::HomeValueIsNotADirectory
            ))
            if path == pyvenv_cfg_path
        ));
    }
}
