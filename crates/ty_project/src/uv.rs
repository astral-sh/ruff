//! Runs uv commands and coordinates project and script environments.

use ruff_db::system::System;
use ty_combine::Combine;
use ty_static::EnvVars;

pub(crate) use command::{MetadataTarget, Uv, uv_executable_error};
pub(crate) use environments::{ProjectEnvironment, ScriptEnvironmentCacheKey, script_environment};
pub use environments::{ScriptEnvironmentAvailability, UvEnvironments, UvSyncChanges};
pub(crate) use metadata::{DependencyMetadataError, UvMetadata, UvMetadataError};
pub(crate) use service::{
    ScriptSyncRequest, ScriptSyncTask, UvMetadataResult, UvMetadataService, UvSyncTask,
};

mod command;
mod environments;
mod metadata;
mod service;

/// Controls which uv integrations ty uses.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    get_size2::GetSize,
    serde::Deserialize,
    serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum UseUv {
    /// Disable all uv integration.
    #[default]
    Off,

    /// Use uv to create environments for standalone scripts.
    ///
    /// This does not use uv for project discovery.
    Scripts,

    /// Use uv for project discovery and standalone script environments.
    On,
}

impl UseUv {
    /// Resolves the mode configured by the `TY_UV` environment variable.
    pub fn from_system(system: &dyn System) -> Self {
        match system.env_var(EnvVars::TY_UV).as_deref() {
            Ok("1" | "true") => Self::On,
            Ok("scripts") => Self::Scripts,
            _ => Self::Off,
        }
    }

    pub(super) const fn workspace_discovery_enabled(self) -> bool {
        matches!(self, Self::On)
    }

    const fn script_environments_enabled(self) -> bool {
        matches!(self, Self::Scripts | Self::On)
    }
}

impl Combine for UseUv {
    fn combine_with(&mut self, other: Self) {
        *self = other;
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::system::TestSystem;
    use ty_static::EnvVars;

    use super::UseUv;

    #[test]
    fn use_uv_from_system() {
        let system = TestSystem::default();
        assert_eq!(UseUv::from_system(&system), UseUv::Off);

        system.set_env_var(EnvVars::TY_UV, "scripts");
        assert_eq!(UseUv::from_system(&system), UseUv::Scripts);

        system.set_env_var(EnvVars::TY_UV, "true");
        assert_eq!(UseUv::from_system(&system), UseUv::On);

        system.set_env_var(EnvVars::TY_UV, "off");
        assert_eq!(UseUv::from_system(&system), UseUv::Off);
    }
}
