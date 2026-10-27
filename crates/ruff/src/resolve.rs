use std::path::Path;

use anyhow::{Result, bail};
use log::debug;

use ruff_workspace::configuration::Configuration;
use ruff_workspace::pyproject::{self, find_fallback_target_version};
use ruff_workspace::resolver::{
    ConfigurationOrigin, PyprojectConfig, PyprojectDiscoveryStrategy, ResolvedSettings,
    resolve_root_settings,
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
        debug!("Isolated mode, not reading any pyproject.toml");
        return Ok(PyprojectConfig {
            strategy: PyprojectDiscoveryStrategy::Fixed,
            base: ResolvedSettings::new(Configuration::default(), config_arguments, &cwd, None)?,
        });
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
        return Ok(PyprojectConfig {
            strategy: PyprojectDiscoveryStrategy::Fixed,
            base: settings,
        });
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
        return Ok(PyprojectConfig {
            strategy: PyprojectDiscoveryStrategy::Hierarchical,
            base: settings,
        });
    }

    // Fourth priority: find a user-specific `pyproject.toml`, but resolve all paths
    // relative the current working directory. (With `Strategy::Hierarchical`, we'll
    // end up the "closest" `pyproject.toml` file for every Python file later on, so
    // these act as the "default" settings.)
    if let Some(pyproject) = pyproject::find_user_settings_toml() {
        debug!(
            "Using configuration file (via cwd) at: {}",
            pyproject.display()
        );
        let settings = resolve_root_settings(
            &pyproject,
            config_arguments,
            ConfigurationOrigin::UserSettings {
                fallback_directory: &cwd,
            },
        )?;
        return Ok(PyprojectConfig {
            strategy: PyprojectDiscoveryStrategy::Hierarchical,
            base: settings,
        });
    }

    // Fallback: load Ruff's default settings, and resolve all paths relative to the
    // current working directory. (With `Strategy::Hierarchical`, we'll end up the
    // "closest" `pyproject.toml` file for every Python file later on, so these act
    // as the "default" settings.)
    debug!("Using Ruff default settings");
    // A `pyproject.toml` without a `[tool.ruff]` section can still specify `requires-python`.
    let fallback = find_fallback_target_version(stdin_filename.unwrap_or(&cwd));
    if let Some(version) = fallback {
        debug!("Derived `target-version` from found `requires-python`: {version:?}");
    }
    let configuration = Configuration {
        target_version: fallback.map(ast::PythonVersion::from),
        ..Configuration::default()
    };
    Ok(PyprojectConfig {
        strategy: PyprojectDiscoveryStrategy::Hierarchical,
        base: ResolvedSettings::new(configuration, config_arguments, &cwd, None)?,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;
    use clap::Parser;
    use ruff_python_ast::PythonVersion;
    use ruff_workspace::resolver::project_files_in_path;
    use tempfile::TempDir;

    use super::resolve;
    use crate::args::{CheckCommand, GlobalConfigArgs};

    #[test]
    fn retains_configuration_before_cli_overrides() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path();
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nrequires-python = '>=3.10'\n[tool.ruff]\n",
        )?;
        let nested = root.join("nested");
        fs::create_dir(&nested)?;
        fs::write(root.join("shared.toml"), "target-version = 'py311'")?;
        fs::write(nested.join("ruff.toml"), "extend = '../shared.toml'")?;

        let (_, arguments) = CheckCommand::try_parse_from(["check", "--target-version", "py312"])?
            .partition(GlobalConfigArgs::default())?;
        let config = resolve(&arguments, Some(root))?;
        let (_, resolver) =
            project_files_in_path(std::slice::from_ref(&nested), &config, &arguments)?;

        for (directory, base_version) in [
            (root, PythonVersion::PY310),
            (nested.as_path(), PythonVersion::PY311),
        ] {
            let resolved = resolver.resolve_with_path(&directory.join("script.py"));
            assert_eq!(resolved.configuration.target_version, Some(base_version));
            assert_eq!(
                resolved.settings.linter.unresolved_target_version.0,
                Some(PythonVersion::PY312)
            );
        }
        Ok(())
    }
}
