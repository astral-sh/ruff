use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use log::debug;
use path_absolutize::path_dedot;

use ruff_workspace::configuration::Configuration;
use ruff_workspace::options::Options;
use ruff_workspace::pyproject::{self, parse_ruff_toml_from_cli};
use ruff_workspace::resolver::{
    resolve_root_settings, ConfigurationTransformer, PyprojectConfig, PyprojectDiscoveryStrategy,
    Relativity
};

use crate::args::{CliOverrides, ConfigOption};

#[derive(Debug, Default)]
struct ConfigArgs<'a> {
    config_file: Option<&'a PathBuf>,
    overrides: Vec<Options>,
}

impl<'a> ConfigArgs<'a> {
    fn from_cli_options(options: &'a Option<Vec<ConfigOption>>) -> Result<Self> {
        let Some(options) = options else {
            return Ok(Self::default());
        };

        let mut new = Self {
            config_file: None,
            overrides: Vec::with_capacity(options.len().saturating_sub(1)),
        };

        for option in options {
            match option {
                ConfigOption::InlineToml(value) => {
                    let overiden_option = parse_ruff_toml_from_cli(value)
                        .with_context(|| format!("The path `{value}` does not exist on the file system"))
                        .with_context(|| "`--config` arguments must either be a path that exists on the file system, or a valid TOML string")
                        .with_context(|| format!("Failed to parse command-line argument `--config=\"{value}\"`"))?;
                    new.overrides.push(overiden_option);
                }
                ConfigOption::PathToConfigFile(path) => {
                    if new.config_file.is_none() {
                        new.config_file = Some(path);
                    } else {
                        bail!("Cannot specify more than one configuration file on the command line")
                    }
                }
            }
        }
        Ok(new)
    }
}

/// Resolve the relevant settings strategy and defaults for the current
/// invocation.
pub fn resolve(
    isolated: bool,
    config_options: &Option<Vec<ConfigOption>>,
    overrides: &CliOverrides,
    stdin_filename: Option<&Path>,
) -> Result<PyprojectConfig> {
    let config_args = ConfigArgs::from_cli_options(config_options)?;

    // First priority: if we're running in isolated mode, use the default settings.
    if isolated {
        let mut config = overrides.transform(Configuration::default());
        let project_root = &path_dedot::CWD;
        for option in config_args.overrides {
            config = config.combine(Configuration::from_options(option, project_root)?);
        }
        let settings = config.into_settings(project_root)?;
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
    if let Some(pyproject) = config_args
        .config_file
        .map(|config| config.display().to_string())
        .map(|config| shellexpand::full(&config).map(|config| PathBuf::from(config.as_ref())))
        .transpose()?
    {
        let settings = resolve_root_settings(&pyproject, Relativity::Cwd, overrides)?;
        debug!(
            "Using user-specified configuration file at: {}",
            pyproject.display()
        );
        return Ok(PyprojectConfig::new(
            PyprojectDiscoveryStrategy::Fixed,
            settings,
            Some(pyproject),
        ));
    }

    // Third priority: find a `pyproject.toml` file in either an ancestor of
    // `stdin_filename` (if set) or the current working path all paths relative to
    // that directory. (With `Strategy::Hierarchical`, we'll end up finding
    // the "closest" `pyproject.toml` file for every Python file later on,
    // so these act as the "default" settings.)
    if let Some(pyproject) = pyproject::find_settings_toml(
        stdin_filename
            .as_ref()
            .unwrap_or(&path_dedot::CWD.as_path()),
    )? {
        debug!(
            "Using configuration file (via parent) at: {}",
            pyproject.display()
        );
        let settings = resolve_root_settings(&pyproject, Relativity::Parent, overrides)?;
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
        debug!(
            "Using configuration file (via cwd) at: {}",
            pyproject.display()
        );
        let settings = resolve_root_settings(&pyproject, Relativity::Cwd, overrides)?;
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
    let config = overrides.transform(Configuration::default());
    let settings = config.into_settings(&path_dedot::CWD)?;
    Ok(PyprojectConfig::new(
        PyprojectDiscoveryStrategy::Hierarchical,
        settings,
        None,
    ))
}
