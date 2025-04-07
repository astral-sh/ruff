use std::path::Path;

use anyhow::{bail, Result};
use log::debug;
use path_absolutize::path_dedot;

use ruff_workspace::configuration::Configuration;
use ruff_workspace::pyproject::{self, find_fallback_target_version};
use ruff_workspace::resolver::{
    resolve_root_settings, ConfigurationOrigin, ConfigurationTransformer, PyprojectConfig,
    PyprojectDiscoveryStrategy,
};

use ruff_python_ast as ast;

use crate::args::ConfigArguments;

/// Resolve the relevant settings strategy and defaults for the current
/// invocation.
pub fn resolve(
    config_arguments: &ConfigArguments,
    stdin_filename: Option<&Path>,
) -> Result<PyprojectConfig> {
    let Ok(cwd) = std::env::current_dir() else {
        bail!("Working directory does not exist")
    };

    // First priority: if we're running in isolated mode, use the default settings.
    if config_arguments.isolated {
        let config = config_arguments.transform(Configuration::default());
        let settings = config.into_settings(&cwd)?;
        debug!("Isolated mode, not reading any pyproject.toml");
        return Ok(PyprojectConfig::new(
            PyprojectDiscoveryStrategy::Fixed,
            settings,
            None,
        ));
    }

    // Second priority: the user specified a `pyproject.toml` file. Use that
    // `pyproject.toml` for _all_ configuration, and resolve paths relative to the
    // current working directory. (This matches ESLint's behavior.)
    if let Some(pyproject) = config_arguments.config_file() {
        let settings = resolve_root_settings(
            pyproject,
            config_arguments,
            ConfigurationOrigin::UserSpecified,
        )?;
        debug!(
            "Using user-specified configuration file at: {}",
            pyproject.display()
        );
        return Ok(PyprojectConfig::new(
            PyprojectDiscoveryStrategy::Fixed,
            settings,
            Some(pyproject.to_path_buf()),
        ));
    }

    // Third priority: find a `pyproject.toml` file in either an ancestor of
    // `stdin_filename` (if set) or the current working path all paths relative to
    // that directory. (With `Strategy::Hierarchical`, we'll end up finding
    // the "closest" `pyproject.toml` file for every Python file later on,
    // so these act as the "default" settings.)
    if let Some(pyproject) = pyproject::find_settings_toml(stdin_filename.unwrap_or(&cwd))? {
        debug!(
            "Using configuration file (via parent) at: {}",
            pyproject.display()
        );
        let settings =
            resolve_root_settings(&pyproject, config_arguments, ConfigurationOrigin::Ancestor)?;
        return Ok(PyprojectConfig::new(
            PyprojectDiscoveryStrategy::Hierarchical,
            settings,
            Some(pyproject),
        ));
    }

    // Fourth priority: find a user-specific `pyproject.toml`, but resolve all paths
    // relative the current working directory. (With `Strategy::Hierarchical`, we'll
    // end up the "closest" `pyproject.toml` file for every Python file later on, so
    // these act as the "default" settings.)
    if let Some(pyproject) = pyproject::find_user_settings_toml() {
        struct FallbackTransformer<'a> {
            arguments: &'a ConfigArguments,
        }

        impl ConfigurationTransformer for FallbackTransformer<'_> {
            fn transform(&self, mut configuration: Configuration) -> Configuration {
                // The `requires-python` constraint from the `pyproject.toml` takes precedence
                // over the `target-version` from the user configuration.
                let fallback = find_fallback_target_version(&*path_dedot::CWD);
                if let Some(fallback) = fallback {
                    debug!("Derived `target-version` from found `requires-python`: {fallback:?}");
                    configuration.target_version = Some(fallback.into());
                }

                self.arguments.transform(configuration)
            }
        }

        debug!(
            "Using configuration file (via cwd) at: {}",
            pyproject.display()
        );
        let settings = resolve_root_settings(
            &pyproject,
            &FallbackTransformer {
                arguments: config_arguments,
            },
            ConfigurationOrigin::UserSettings,
        )?;
        return Ok(PyprojectConfig::new(
            PyprojectDiscoveryStrategy::Hierarchical,
            settings,
            Some(pyproject),
        ));
    }

    // Fallback: load Ruff's default settings, and resolve all paths relative to the
    // current working directory. (With `Strategy::Hierarchical`, we'll end up the
    // "closest" `pyproject.toml` file for every Python file later on, so these act
    // as the "default" settings.)
    debug!("Using Ruff default settings");
    let mut config = config_arguments.transform(Configuration::default());
    if config.target_version.is_none() {
        // If we have arrived here we know that there was no `pyproject.toml`
        // containing a `[tool.ruff]` section found in an ancestral directory.
        // (This is an implicit requirement in the function
        // `pyproject::find_settings_toml`.)
        // However, there may be a `pyproject.toml` with a `requires-python`
        // specified, and that is what we look for in this step.
        let fallback = find_fallback_target_version(stdin_filename.unwrap_or(&cwd));
        if let Some(version) = fallback {
            debug!("Derived `target-version` from found `requires-python`: {version:?}");
        }
        config.target_version = fallback.map(ast::PythonVersion::from);
    }
    let settings = config.into_settings(&cwd)?;
    Ok(PyprojectConfig::new(
        PyprojectDiscoveryStrategy::Hierarchical,
        settings,
        None,
    ))
}
