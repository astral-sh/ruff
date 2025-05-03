use std::sync::Arc;

use ruff_db::system::{System, SystemPath, SystemPathBuf};
use thiserror::Error;

use crate::metadata::value::ValueSource;

use super::options::{Options, TyTomlError};

/// A `ty.toml` configuration file with the options it contains.
pub(crate) struct ConfigurationFile {
    path: SystemPathBuf,
    options: Options,
}

impl ConfigurationFile {
    /// Loads the user-level configuration file if it exists.
    ///
    /// Returns `None` if the file does not exist or if the concept of user-level configurations
    /// doesn't exist on `system`.
    pub(crate) fn user(system: &dyn System) -> Result<Option<Self>, ConfigurationFileError> {
        let Some(configuration_directory) = system.user_config_directory() else {
            return Ok(None);
        };

        let ty_toml_path = configuration_directory.join("ty").join("ty.toml");

        tracing::debug!(
            "Searching for a user-level configuration at `{path}`",
            path = &ty_toml_path
        );

        let Ok(ty_toml_str) = system.read_to_string(&ty_toml_path) else {
            return Ok(None);
        };

        match Options::from_toml_str(
            &ty_toml_str,
            ValueSource::File(Arc::new(ty_toml_path.clone())),
        ) {
            Ok(options) => Ok(Some(Self {
                path: ty_toml_path,
                options,
            })),
            Err(error) => Err(ConfigurationFileError::InvalidTyToml {
                source: Box::new(error),
                path: ty_toml_path,
            }),
        }
    }

    /// Returns the path to the configuration file.
    pub(crate) fn path(&self) -> &SystemPath {
        &self.path
    }

    pub(crate) fn into_options(self) -> Options {
        self.options
    }
}

#[derive(Debug, Error)]
pub enum ConfigurationFileError {
    #[error("{path} is not a valid `ty.toml`: {source}")]
    InvalidTyToml {
        source: Box<TyTomlError>,
        path: SystemPathBuf,
    },
}
