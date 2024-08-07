//! Utilities for finding the `site-packages` directory,
//! into which third-party packages are installed.
//!
//! The routines exposed by this module have different behaviour depending
//! on the platform of the *host machine*, which may be
//! different from the *target platform for type checking*. (A user
//! might be running red-knot on a Windows machine, but might
//! reasonably ask us to type-check code assuming that the code runs
//! on Linux.)

use std::io;

use ruff_db::system::{System, SystemPath, SystemPathBuf};

/// Attempt to retrieve the `site-packages` directory
/// associated with a given Python installation.
///
/// `sys_prefix_path` is equivalent to the value of [`sys.prefix`]
/// at runtime in Python. For the case of a virtual environment, where a
/// Python binary is at `/.venv/bin/python`, `sys.prefix` is the path to
/// the virtual environment the Python binary lies inside, i.e. `/.venv`,
/// and `site-packages` will be at `.venv/lib/python3.X/site-packages`.
/// System Python installations generally work the same way: if a system
/// Python installation lies at `/opt/homebrew/bin/python`, `sys.prefix`
/// will be `/opt/homebrew`, and `site-packages` will be at
/// `/opt/homebrew/lib/python3.X/site-packages`.
///
/// This routine does not verify that `sys_prefix_path` points
/// to an existing directory on disk; it is assumed that this has already
/// been checked.
///
/// [`sys.prefix`]: https://docs.python.org/3/library/sys.html#sys.prefix
fn site_packages_dir_from_sys_prefix(
    sys_prefix_path: &SystemPath,
    system: &dyn System,
) -> Result<SystemPathBuf, SitePackagesDiscoveryError> {
    tracing::debug!("Searching for site-packages directory in '{sys_prefix_path}'");

    if cfg!(target_os = "windows") {
        let site_packages = sys_prefix_path.join("Lib/site-packages");

        return if system.is_directory(&site_packages) {
            tracing::debug!("Resolved site-packages directory to '{site_packages}'");
            Ok(site_packages)
        } else {
            Err(SitePackagesDiscoveryError::NoSitePackagesDirFound)
        };
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
    for entry_result in system.read_directory(&sys_prefix_path.join("lib"))? {
        let Ok(entry) = entry_result else {
            continue;
        };

        if !entry.file_type().is_directory() {
            continue;
        }

        let mut path = entry.into_path();

        // The `python3.x` part of the `site-packages` path can't be computed from
        // the `--target-version` the user has passed, as they might be running Python 3.12 locally
        // even if they've requested that we type check their code "as if" they're running 3.8.
        //
        // The `python3.x` part of the `site-packages` path *could* be computed
        // by parsing the virtual environment's `pyvenv.cfg` file.
        // Right now that seems like overkill, but in the future we may need to parse
        // the `pyvenv.cfg` file anyway, in which case we could switch to that method
        // rather than iterating through the whole directory until we find
        // an entry where the last component of the path starts with `python3.`
        let name = path
            .file_name()
            .expect("File name to be non-null because path is guaranteed to be a child of `lib`");

        if !name.starts_with("python3.") {
            continue;
        }

        path.push("site-packages");
        if system.is_directory(&path) {
            tracing::debug!("Resolved site-packages directory to '{path}'");
            return Ok(path);
        }
    }
    Err(SitePackagesDiscoveryError::NoSitePackagesDirFound)
}

#[derive(Debug, thiserror::Error)]
pub enum SitePackagesDiscoveryError {
    #[error("Invalid --venv-path argument: {0} does not point to a directory on disk")]
    VenvDirIsNotADirectory(SystemPathBuf),
    #[error("--venv-path points to a broken venv with no pyvenv.cfg file")]
    NoPyvenvCfgFile(#[source] io::Error),
    #[error("Failed to parse the pyvenv.cfg file at {0}")]
    PyvenvCfgParseError(SystemPathBuf),
    #[error("Failed to search the virtual environment directory for `site-packages`")]
    CouldNotReadLibDirectory(#[from] io::Error),
    #[error("Could not find the `site-packages` directory in the virtual environment")]
    NoSitePackagesDirFound,
}

pub struct VirtualEnvironment {
    venv_path: SystemPathBuf,
    base_executable_path: SystemPathBuf,
    include_system_site_packages: bool,
    minor_version: u8,
}

impl VirtualEnvironment {
    pub fn new(
        path: SystemPathBuf,
        system: &dyn System,
    ) -> Result<Self, SitePackagesDiscoveryError> {
        let canonicalized_venv = system
            .canonicalize_path(&path)
            .unwrap_or_else(|_| path.clone());
        if !system.is_directory(&canonicalized_venv) {
            return Err(SitePackagesDiscoveryError::VenvDirIsNotADirectory(path));
        }

        let pyvenv_cfg_path = canonicalized_venv.join("pyvenv.cfg");
        let pyvenv_cfg = system
            .read_to_string(&pyvenv_cfg_path)
            .map_err(SitePackagesDiscoveryError::NoPyvenvCfgFile)?;

        let mut include_system_site_packages = false;
        let mut base_executable_path = None;
        let mut version_info_string = None;

        for line in pyvenv_cfg.lines() {
            let mut line_split = line.split('=');
            if let Some(key) = line_split.next() {
                let (Some(value), None) = (line_split.next(), line_split.next()) else {
                    return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                        pyvenv_cfg_path,
                    ));
                };
                let key = key.trim();
                let value = value.trim();
                match key {
                    "include-system-site-package" => {
                        include_system_site_packages = value.trim().eq_ignore_ascii_case("true");
                    }
                    "home" => base_executable_path = Some(value),
                    "version_info" => version_info_string = Some(value),
                    _ => continue,
                }
            }
        }

        let Some(base_executable_path) = base_executable_path else {
            return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                pyvenv_cfg_path,
            ));
        };
        let base_executable_path = system
            .canonicalize_path(SystemPath::new(base_executable_path))
            .unwrap_or_else(|_| SystemPathBuf::from(base_executable_path));
        if !system.is_directory(&base_executable_path) {
            return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                pyvenv_cfg_path,
            ));
        }

        let Some(version_info_string) = version_info_string else {
            return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                pyvenv_cfg_path,
            ));
        };
        let mut version_info_parts = version_info_string.split('.');
        let (Some("3"), Some(minor)) = (version_info_parts.next(), version_info_parts.next())
        else {
            return Err(SitePackagesDiscoveryError::PyvenvCfgParseError(
                pyvenv_cfg_path,
            ));
        };
        let minor_version = minor
            .parse()
            .map_err(|_| SitePackagesDiscoveryError::PyvenvCfgParseError(pyvenv_cfg_path))?;

        Ok(Self {
            venv_path: canonicalized_venv,
            base_executable_path,
            include_system_site_packages,
            minor_version,
        })
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
            base_executable_path,
            include_system_site_packages,
            minor_version,
        } = self;

        let mut site_packages_dirs = vec![site_packages_dir_from_sys_prefix(venv_path, system)?];
        if *include_system_site_packages {
            site_packages_dirs.push(site_packages_dir_from_sys_prefix(
                base_executable_path,
                system,
            )?);
        }
        Ok(site_packages_dirs)
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::system::OsSystem;

    use super::*;

    #[test]
    // Windows venvs have different layouts, and we only have a Unix venv committed for now.
    // This test is skipped on Windows until we commit a Windows venv.
    #[cfg_attr(target_os = "windows", ignore = "Windows has a different venv layout")]
    fn can_find_site_packages_dir_in_committed_venv() {
        let path_to_venv = SystemPathBuf::from("resources/test/unix-uv-venv");
        let system = OsSystem::default();

        // if this doesn't hold true, the premise of the test is incorrect.
        assert!(system.is_directory(&path_to_venv));

        let virtual_environment = VirtualEnvironment::new(path_to_venv, &system).unwrap();
        let site_packages_dirs = virtual_environment.site_packages_dirs(&system).unwrap();
        assert_eq!(site_packages_dirs.len(), 1);
    }
}
