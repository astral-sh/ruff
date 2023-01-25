use std::path::Path;

use anyhow::Result;
use path_absolutize::path_dedot;
use ruff::resolver::{
    resolve_settings_with_processor, ConfigProcessor, PyprojectDiscovery, Relativity,
};
use ruff::settings::configuration::Configuration;
use ruff::settings::{pyproject, AllSettings};

use crate::args::Overrides;

/// Resolve the relevant settings strategy and defaults for the current
/// invocation.
pub fn resolve(
    isolated: bool,
    config: Option<&Path>,
    overrides: &Overrides,
    stdin_filename: Option<&Path>,
) -> Result<PyprojectDiscovery> {
    if isolated {
        // First priority: if we're running in isolated mode, use the default settings.
        let mut config = Configuration::default();
        overrides.process_config(&mut config);
        let settings = AllSettings::from_configuration(config, &path_dedot::CWD)?;
        Ok(PyprojectDiscovery::Fixed(settings))
    } else if let Some(pyproject) = config {
        // Second priority: the user specified a `pyproject.toml` file. Use that
        // `pyproject.toml` for _all_ configuration, and resolve paths relative to the
        // current working directory. (This matches ESLint's behavior.)
        let settings = resolve_settings_with_processor(pyproject, &Relativity::Cwd, overrides)?;
        Ok(PyprojectDiscovery::Fixed(settings))
    } else if let Some(pyproject) = pyproject::find_settings_toml(
        stdin_filename
            .as_ref()
            .unwrap_or(&path_dedot::CWD.as_path()),
    )? {
        // Third priority: find a `pyproject.toml` file in either an ancestor of
        // `stdin_filename` (if set) or the current working path all paths relative to
        // that directory. (With `Strategy::Hierarchical`, we'll end up finding
        // the "closest" `pyproject.toml` file for every Python file later on,
        // so these act as the "default" settings.)
        let settings = resolve_settings_with_processor(&pyproject, &Relativity::Parent, overrides)?;
        Ok(PyprojectDiscovery::Hierarchical(settings))
    } else if let Some(pyproject) = pyproject::find_user_settings_toml() {
        // Fourth priority: find a user-specific `pyproject.toml`, but resolve all paths
        // relative the current working directory. (With `Strategy::Hierarchical`, we'll
        // end up the "closest" `pyproject.toml` file for every Python file later on, so
        // these act as the "default" settings.)
        let settings = resolve_settings_with_processor(&pyproject, &Relativity::Cwd, overrides)?;
        Ok(PyprojectDiscovery::Hierarchical(settings))
    } else {
        // Fallback: load Ruff's default settings, and resolve all paths relative to the
        // current working directory. (With `Strategy::Hierarchical`, we'll end up the
        // "closest" `pyproject.toml` file for every Python file later on, so these act
        // as the "default" settings.)
        let mut config = Configuration::default();
        overrides.process_config(&mut config);
        let settings = AllSettings::from_configuration(config, &path_dedot::CWD)?;
        Ok(PyprojectDiscovery::Hierarchical(settings))
    }
}
