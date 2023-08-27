//! Discover Python files, and their corresponding [`Settings`], from the
//! filesystem.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use path_absolutize::path_dedot;

use crate::fs;
use crate::settings::{AllSettings, Settings};

/// The configuration information from a `pyproject.toml` file.
pub struct PyprojectConfig {
    /// The strategy used to discover the relevant `pyproject.toml` file for
    /// each Python file.
    pub strategy: PyprojectDiscoveryStrategy,
    /// All settings from the `pyproject.toml` file.
    pub settings: AllSettings,
    /// Absolute path to the `pyproject.toml` file. This would be `None` when
    /// either using the default settings or the `--isolated` flag is set.
    pub path: Option<PathBuf>,
}

impl PyprojectConfig {
    pub fn new(
        strategy: PyprojectDiscoveryStrategy,
        settings: AllSettings,
        path: Option<PathBuf>,
    ) -> Self {
        Self {
            strategy,
            settings,
            path: path.map(fs::normalize_path),
        }
    }
}

/// The strategy used to discover the relevant `pyproject.toml` file for each
/// Python file.
#[derive(Debug, is_macro::Is)]
pub enum PyprojectDiscoveryStrategy {
    /// Use a fixed `pyproject.toml` file for all Python files (i.e., one
    /// provided on the command-line).
    Fixed,
    /// Use the closest `pyproject.toml` file in the filesystem hierarchy, or
    /// the default settings.
    Hierarchical,
}

/// The strategy for resolving file paths in a `pyproject.toml`.
#[derive(Copy, Clone)]
pub enum Relativity {
    /// Resolve file paths relative to the current working directory.
    Cwd,
    /// Resolve file paths relative to the directory containing the
    /// `pyproject.toml`.
    Parent,
}

impl Relativity {
    pub fn resolve(&self, path: &Path) -> PathBuf {
        match self {
            Relativity::Parent => path
                .parent()
                .expect("Expected pyproject.toml file to be in parent directory")
                .to_path_buf(),
            Relativity::Cwd => path_dedot::CWD.clone(),
        }
    }
}

#[derive(Default)]
pub struct Resolver {
    settings: BTreeMap<PathBuf, AllSettings>,
}

impl Resolver {
    /// Add a resolved [`Settings`] under a given [`PathBuf`] scope.
    pub fn add(&mut self, path: PathBuf, settings: AllSettings) {
        self.settings.insert(path, settings);
    }

    /// Return the appropriate [`AllSettings`] for a given [`Path`].
    pub fn resolve_all<'a>(
        &'a self,
        path: &Path,
        pyproject_config: &'a PyprojectConfig,
    ) -> &'a AllSettings {
        match pyproject_config.strategy {
            PyprojectDiscoveryStrategy::Fixed => &pyproject_config.settings,
            PyprojectDiscoveryStrategy::Hierarchical => self
                .settings
                .iter()
                .rev()
                .find_map(|(root, settings)| path.starts_with(root).then_some(settings))
                .unwrap_or(&pyproject_config.settings),
        }
    }

    pub fn resolve<'a>(
        &'a self,
        path: &Path,
        pyproject_config: &'a PyprojectConfig,
    ) -> &'a Settings {
        &self.resolve_all(path, pyproject_config).lib
    }

    /// Return an iterator over the resolved [`Settings`] in this [`Resolver`].
    pub fn iter(&self) -> impl Iterator<Item = &AllSettings> {
        self.settings.values()
    }
}
