//! Runs uv commands and coordinates project and script environments.

use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::system::System;
use ruff_ranged_value::{RangedValue, ValueSource};
use ty_combine::Combine;
use ty_static::EnvVars;

use crate::Db;

pub(crate) use command::{MetadataTarget, Uv};
pub use environments::{ScriptEnvironmentAvailability, UvEnvironments, UvSyncChanges, UvWorkspace};
pub(crate) use environments::{ScriptEnvironmentCacheKey, script_environment};
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
    pub fn from_system(system: &dyn System) -> Option<Self> {
        match system.env_var(EnvVars::TY_UV).as_deref() {
            Ok("1" | "true") => Some(Self::On),
            Ok("scripts") => Some(Self::Scripts),
            Err(std::env::VarError::NotPresent) => None,
            _ => Some(Self::Off),
        }
    }
}

impl Combine for UseUv {
    fn combine_with(&mut self, other: Self) {
        *self = other;
    }
}

/// Explain why ty tried to run uv, using the setting's current source and range.
pub(crate) fn add_uv_setting_info<T>(
    db: &dyn Db,
    diagnostic: &mut Diagnostic,
    setting: &RangedValue<T>,
    option: &str,
    purpose: &str,
) {
    let source = setting.source();
    let reason = match source {
        ValueSource::ScriptMetadata(_) | ValueSource::File(_) => {
            format_args!("of your configuration")
        }
        ValueSource::Cli if UseUv::from_system(db.system()).is_some() => {
            format_args!("the `TY_UV` environment variable enables it")
        }
        ValueSource::Cli => format_args!("`{option}` was set to `\"uv\"` on the command line"),
        ValueSource::Editor => format_args!("it was enabled in your editor settings"),
        ValueSource::UvMetadata => return,
    };

    let info = diagnostic.prepend_info(format_args!("ty uses uv to {purpose} because {reason}"));
    if let Some(file) = source.file(db) {
        info.annotate(
            Annotation::primary(Span::from(file).with_optional_range(setting.range()))
                .message("uv specified here"),
        );
    }
}

/// Environment variables for uv integration tests.
///
/// Tests use this allowlist after clearing the inherited environment so host settings such as
/// `UV_LOCKED` and `PYTHONPATH` cannot affect subprocesses, while retaining executable, Python,
/// and cache discovery. A fixed resolution cutoff prevents tests from executing newly published
/// packages, including build dependencies.
#[allow(
    clippy::disallowed_methods,
    reason = "Test only code, intentionally inherit variables from the host's environment."
)]
pub fn uv_test_env_vars() -> impl Iterator<Item = (&'static str, String)> {
    [
        "PATH",
        "PATHEXT",
        "SYSTEMROOT",
        "HOME",
        "USERPROFILE",
        "XDG_DATA_HOME",
        "TMPDIR",
        "TEMP",
        "TMP",
        "UV_CACHE_DIR",
        "UV_PYTHON_INSTALL_DIR",
    ]
    .into_iter()
    .filter_map(|name| std::env::var(name).ok().map(|value| (name, value)))
    .chain([("UV_EXCLUDE_NEWER", "2026-09-22T00:00:00Z".to_owned())])
}

#[cfg(test)]
mod tests {
    use ruff_db::system::TestSystem;
    use ty_static::EnvVars;

    use super::UseUv;

    #[test]
    fn use_uv_from_system() {
        let system = TestSystem::default();
        assert_eq!(UseUv::from_system(&system), None);

        system.set_env_var(EnvVars::TY_UV, "scripts");
        assert_eq!(UseUv::from_system(&system), Some(UseUv::Scripts));

        system.set_env_var(EnvVars::TY_UV, "true");
        assert_eq!(UseUv::from_system(&system), Some(UseUv::On));

        system.set_env_var(EnvVars::TY_UV, "off");
        assert_eq!(UseUv::from_system(&system), Some(UseUv::Off));
    }
}
