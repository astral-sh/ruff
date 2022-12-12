//! Discover and resolve `Settings` from the filesystem hierarchy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::cli::Overrides;
use crate::settings::configuration::Configuration;
use crate::settings::{pyproject, Settings};

pub enum Strategy {
    Fixed,
    Hierarchical,
}

#[derive(Default)]
pub struct Resolver {
    settings: BTreeMap<PathBuf, Settings>,
}

impl Resolver {
    pub fn merge(&mut self, resolver: Resolver) {
        self.settings.extend(resolver.settings);
    }

    pub fn add(&mut self, path: PathBuf, settings: Settings) {
        self.settings.insert(path, settings);
    }

    pub fn resolve(&self, path: &Path, strategy: &Strategy) -> Option<&Settings> {
        match strategy {
            Strategy::Fixed => None,
            Strategy::Hierarchical => self.settings.iter().rev().find_map(|(root, settings)| {
                if path.starts_with(root) {
                    Some(settings)
                } else {
                    None
                }
            }),
        }
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
    configuration.merge(overrides.clone());
    let settings = Settings::from_configuration(configuration, &project_root)?;
    Ok((project_root, settings))
}
