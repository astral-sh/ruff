//! Discover and resolve `Settings` from the filesystem hierarchy.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use log::error;

use crate::cli::Overrides;
use crate::fs::iter_pyproject_files;
use crate::settings::configuration::Configuration;
use crate::settings::{pyproject, Settings};

pub struct Resolver<'a> {
    pub settings: &'a [(PathBuf, Settings)],
    pub default: &'a Settings,
}

impl<'a> Resolver<'a> {
    pub fn resolve(&'a self, path: &Path) -> &'a Settings {
        self.settings
            .iter()
            .find(|(root, _)| path.starts_with(root))
            .map_or(self.default, |(_, settings)| settings)
    }
}

/// Extract the `Settings` from a given `pyproject.toml`.
pub fn settings_for_path(pyproject: &Path, overrides: &Overrides) -> Result<(PathBuf, Settings)> {
    let project_root = pyproject
        .parent()
        .ok_or_else(|| anyhow!("Expected pyproject.toml to be in a directory"))?
        .to_path_buf();
    let options = pyproject::load_options(pyproject)?;
    let mut configuration = Configuration::from_options(options)?;
    configuration.merge(overrides);
    let settings = Settings::from_configuration(configuration, Some(&project_root))?;
    Ok((project_root, settings))
}

/// Discover all `Settings` objects within the relevant filesystem hierarchy.
pub fn discover_settings(files: &[PathBuf], overrides: &Overrides) -> Vec<(PathBuf, Settings)> {
    // Collect all `pyproject.toml` files.
    let mut pyprojects: Vec<PathBuf> = files
        .iter()
        .flat_map(|path| iter_pyproject_files(path))
        .collect();
    pyprojects.sort_unstable_by_key(|path| Reverse(path.to_string_lossy().len()));
    pyprojects.dedup();

    // Read every `pyproject.toml`.
    pyprojects
        .into_iter()
        .filter_map(|pyproject| match settings_for_path(&pyproject, overrides) {
            Ok((project_root, settings)) => Some((project_root, settings)),
            Err(error) => {
                error!("Failed to read settings: {error}");
                None
            }
        })
        .collect::<Vec<_>>()
}
