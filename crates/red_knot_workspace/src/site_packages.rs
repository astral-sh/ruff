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
    for entry_result in system.read_directory(&sys_prefix_path.join("lib"))? {
        let Ok(entry) = entry_result else {
            continue;
        };
        if !entry.file_type().is_directory() {
            continue;
        }

        let path = entry.path();

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
        if !path
            .components()
            .next_back()
            .is_some_and(|last_part| last_part.as_str().starts_with("python3."))
        {
            continue;
        }

        let site_packages_candidate = path.join("site-packages");
        if system.is_directory(&site_packages_candidate) {
            return Ok(site_packages_candidate);
        }
    }
    Err(SitePackagesDiscoveryError::NoSitePackagesDirFound)
}

#[derive(Debug, thiserror::Error)]
pub enum SitePackagesDiscoveryError {
    #[error("Failed to search the virtual environment directory for `site-packages` due to {0}")]
    CouldNotReadLibDirectory(#[from] io::Error),
    #[error("Could not find the `site-packages` directory in the virtual environment")]
    NoSitePackagesDirFound,
}

/// Given a validated, canonicalized path to a virtual environment,
/// return a list of `site-packages` directories that are available from that environment.
///
/// See the documentation for `site_packages_dir_from_sys_prefix` for more details.
///
/// TODO: Currently we only ever return 1 path from this function:
/// the `site-packages` directory that is actually inside the virtual environment.
/// Some `site-packages` directories are able to also access system `site-packages` and
/// user `site-packages`, however.
pub fn site_packages_dirs_of_venv(
    venv_path: &SystemPath,
    system: &dyn System,
) -> Result<Vec<SystemPathBuf>, SitePackagesDiscoveryError> {
    Ok(vec![site_packages_dir_from_sys_prefix(venv_path, system)?])
}

#[cfg(test)]
mod tests {
    use ruff_db::system::{OsSystem, System, SystemPath};

    use crate::site_packages::site_packages_dirs_of_venv;

    #[test]
    // Windows venvs have different layouts, and we only have a Unix venv committed for now.
    // This test is skipped on Windows until we commit a Windows venv.
    #[cfg(not(target_os = "windows"))]
    fn can_find_site_packages_dir_in_committed_venv() {
        let path_to_venv = SystemPath::new("resources/test/unix-uv-venv");
        let system = OsSystem::default();

        // if this doesn't hold true, the premise of the test is incorrect.
        assert!(system.is_directory(path_to_venv));

        let site_packages_dirs = site_packages_dirs_of_venv(path_to_venv, &system).unwrap();
        assert_eq!(site_packages_dirs.len(), 1);
    }
}
