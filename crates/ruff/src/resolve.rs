use std::path::Path;

use anyhow::{Result, bail};
use log::debug;

use ruff_workspace::configuration::Configuration;
use ruff_workspace::pyproject::{self, find_fallback_target_version};
use ruff_workspace::resolver::{
    ConfigurationOrigin, Project, PyprojectDiscoveryStrategy, RootProject, resolve_root_settings,
};

use ruff_python_ast as ast;

use crate::args::ConfigArguments;

/// Resolve the relevant settings strategy and defaults for the current
/// invocation.
pub fn resolve(
    config_arguments: &ConfigArguments,
    stdin_filename: Option<&Path>,
) -> Result<RootProject> {
    let Ok(cwd) = std::env::current_dir() else {
        bail!("Working directory does not exist")
    };

    // First priority: if we're running in isolated mode, use the default settings.
    if config_arguments.isolated {
        debug!("Isolated mode, not reading any pyproject.toml");
        return Ok(RootProject {
            strategy: PyprojectDiscoveryStrategy::Fixed,
            project: Project::resolve(Configuration::default(), &cwd, config_arguments)?,
        });
    }

    // Second priority: the user specified a `pyproject.toml` file. Use that
    // `pyproject.toml` for _all_ configuration, and resolve paths relative to the
    // current working directory. (This matches ESLint's behavior.)
    if let Some(pyproject) = config_arguments.config_file() {
        let project = resolve_root_settings(
            pyproject,
            config_arguments,
            ConfigurationOrigin::UserSpecified,
        )?;
        debug!(
            "Using user-specified configuration file at: {}",
            pyproject.display()
        );
        return Ok(RootProject {
            strategy: PyprojectDiscoveryStrategy::Fixed,
            project,
        });
    }

    // Third priority: find a `pyproject.toml` file in either an ancestor of
    // `stdin_filename` (if set) or the current working directory, and resolve all
    // paths relative to the directory containing that file. (With
    // `PyprojectDiscoveryStrategy::Hierarchical`, we'll end up finding the "closest"
    // `pyproject.toml` file for every Python file later on, so these act as the
    // "default" settings.)
    if let Some(pyproject) = pyproject::find_settings_toml(stdin_filename.unwrap_or(&cwd))? {
        debug!(
            "Using configuration file (via parent) at: {}",
            pyproject.display()
        );
        let project =
            resolve_root_settings(&pyproject, config_arguments, ConfigurationOrigin::Ancestor)?;
        return Ok(RootProject {
            strategy: PyprojectDiscoveryStrategy::Hierarchical,
            project,
        });
    }

    // Fourth priority: find a user-specific `pyproject.toml`, but resolve all paths
    // relative to the current working directory. (With `PyprojectDiscoveryStrategy::Hierarchical`,
    // we'll end up finding the "closest" `pyproject.toml` file for every Python file later on,
    // so these act as the "default" settings.)
    if let Some(pyproject) = pyproject::find_user_settings_toml() {
        debug!(
            "Using configuration file (via cwd) at: {}",
            pyproject.display()
        );
        let project = resolve_root_settings(
            &pyproject,
            config_arguments,
            ConfigurationOrigin::UserSettings {
                fallback_directory: &cwd,
            },
        )?;
        return Ok(RootProject {
            strategy: PyprojectDiscoveryStrategy::Hierarchical,
            project,
        });
    }

    // Fallback: load Ruff's default settings, and resolve all paths relative to the
    // current working directory. (With `PyprojectDiscoveryStrategy::Hierarchical`, we'll
    // end up finding the "closest" `pyproject.toml` file for every Python file later on,
    // so these act as the "default" settings.)
    debug!("Using Ruff default settings");
    let mut configuration = Configuration::default();
    if config_arguments.target_version().is_none() {
        // If we have arrived here, we know that there was no `pyproject.toml`
        // containing a `[tool.ruff]` section found in an ancestral directory.
        // (This is an implicit requirement in the function
        // `pyproject::find_settings_toml`.)
        // However, there may be a `pyproject.toml` with a `requires-python`
        // specified, and that is what we look for in this step.
        let fallback = find_fallback_target_version(stdin_filename.unwrap_or(&cwd));
        if let Some(version) = fallback {
            debug!("Derived `target-version` from found `requires-python`: {version:?}");
        }
        configuration.target_version = fallback.map(ast::PythonVersion::from);
    }
    Ok(RootProject {
        strategy: PyprojectDiscoveryStrategy::Hierarchical,
        project: Project::resolve(configuration, &cwd, config_arguments)?,
    })
}
