//! Expose the host environment to the resolver.

use std::path::PathBuf;

use crate::python_platform::PythonPlatform;
use crate::python_version::PythonVersion;

/// A trait to expose the host environment to the resolver.
pub(crate) trait Host {
    /// The search paths to use when resolving Python modules.
    fn python_search_paths(&self) -> Vec<PathBuf>;

    /// The Python version to use when resolving Python modules.
    fn python_version(&self) -> PythonVersion;

    /// The OS platform to use when resolving Python modules.
    fn python_platform(&self) -> PythonPlatform;
}

/// A host that exposes a fixed set of search paths.
pub(crate) struct StaticHost {
    search_paths: Vec<PathBuf>,
}

impl StaticHost {
    pub(crate) fn new(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }
}

impl Host for StaticHost {
    fn python_search_paths(&self) -> Vec<PathBuf> {
        self.search_paths.clone()
    }

    fn python_version(&self) -> PythonVersion {
        PythonVersion::Py312
    }

    fn python_platform(&self) -> PythonPlatform {
        PythonPlatform::Darwin
    }
}
