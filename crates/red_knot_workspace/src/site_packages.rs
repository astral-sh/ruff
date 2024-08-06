use std::io;

use ruff_db::system::{System, SystemPath, SystemPathBuf};

/// Enumeration of the kinds of platform we might be running on.
///
/// For the purposes of discovering `site-packages` directories,
/// there is only one distinction that matters here: whether or not
/// we're running on windows.
///
/// Note: this is the platform of the *host machine*, which may be
/// different from the *target platform for type checking*. (A user
/// might be running red-knot on a Windows machine, but might
/// reasonably ask us to type-check code assuming that the code runs
/// on Linux.)
#[derive(Copy, Clone, Eq, PartialEq)]
enum HostPlatform {
    Windows,
    Unix,
}

impl HostPlatform {
    fn of_host() -> Self {
        match std::env::consts::OS {
            "windows" => Self::Windows,
            _ => Self::Unix,
        }
    }

    /// Attempt to retrieve the `site-packages` directory that is actually
    /// inside the virtual environment.
    ///
    /// It is assumed that it has already been checked that `venv_path` points
    /// to an existing directory on disk.
    fn venv_site_packages_dir(
        self,
        venv_path: &SystemPath,
        system: &dyn System,
    ) -> Result<SystemPathBuf, SitePackagesDiscoveryError> {
        match self {
            Self::Windows => {
                let site_packages = venv_path.join("Lib/site-packages");
                system
                    .is_directory(&site_packages)
                    .then_some(site_packages)
                    .ok_or(SitePackagesDiscoveryError::NoSitePackagesDirFound)
            }

            // In the Python standard library's `site.py` module
            // (used for finding `site-packages` at runtime),
            // we can find this in [the non-Windows branch]:
            //
            // ```py
            // libdirs = [sys.platlibdir]
            // if sys.platlibdir != "lib":
            //     libdirs.append("lib")
            // ```
            //
            // Pyright therefore searches for both a `lib/python3.10/site-packages` directory
            // and a `lib64/python3.10/site-packages` directory on non-MacOS Unix systems,
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
            Self::Unix => system
                .read_directory(&venv_path.join("lib"))?
                .flatten()
                .find_map(|entry| {
                    if !entry.file_type().is_directory() {
                        return None;
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
                    path.components()
                        .next_back()
                        .is_some_and(|last_part| last_part.as_str().starts_with("python3."))
                        .then_some(path.join("site-packages"))
                })
                .ok_or(SitePackagesDiscoveryError::NoSitePackagesDirFound),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SitePackagesDiscoveryError {
    #[error("Failed to search the virtual environment directory for `site-packages` due to {0}")]
    CouldNotReadLibDirectory(#[from] io::Error),
    #[error("Could not find the `site-packages` directory in the virtual environment")]
    NoSitePackagesDirFound,
}

/// Given a validated, canonicalized path to virtual environment,
/// return a list of `site-packages` directories that are available from that environment.
///
/// TODO: Currently we only ever return 1 path from this function:
/// the `site-packages` directory that is actually inside the virtual environment.
/// Some `site-packages` directories are able to also access system `site-packages` and
/// user `site-packages`, however.
pub fn site_packages_dirs_of_venv(
    venv_path: &SystemPath,
    system: &dyn System,
) -> Result<Vec<SystemPathBuf>, SitePackagesDiscoveryError> {
    Ok(vec![
        HostPlatform::of_host().venv_site_packages_dir(venv_path, system)?
    ])
}
