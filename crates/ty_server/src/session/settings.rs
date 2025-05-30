use std::{ops::Deref, path::PathBuf};

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;

/// Maps a workspace URI to its associated client settings. Used during server initialization.
pub(crate) type WorkspaceSettingsMap = FxHashMap<Url, ClientSettings>;

// TODO(dhruvmanila): We need to mirror the "python.*" namespace on the server side but ideally it
// would be useful to instead use `workspace/didChangeConfiguration` notification instead. This
// would be then used to get all settings and not just the ones in "python.*".

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct Python {
    ty: Option<Ty>,
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct Ty {
    disable_language_services: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct Completions {
    enable: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Experimental {
    completions: Option<Completions>,
}

impl Experimental {
    /// Returns `true` if completions are enabled in the settings.
    pub(crate) fn is_completions_enabled(&self) -> bool {
        self.completions
            .as_ref()
            .is_some_and(|completions| completions.enable.unwrap_or_default())
    }
}

/// This is a direct representation of the settings schema sent by the client.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub struct ClientSettings {
    pub(crate) experimental: Option<Experimental>,

    /// Settings under the `python.*` namespace in VS Code that are useful for the ty language
    /// server.
    python: Option<Python>,

    // These settings are only needed for tracing, and are only read from the global configuration.
    // These will not be in the resolved settings.
    #[serde(flatten)]
    pub(crate) tracing: TracingSettings,
}

/// Settings needed to initialize tracing. These will only be
/// read from the global configuration.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TracingSettings {
    pub(crate) log_level: Option<crate::logging::LogLevel>,
    /// Path to the log file - tildes and environment variables are supported.
    pub(crate) log_file: Option<PathBuf>,
}

/// This is a direct representation of the workspace settings schema,
/// which inherits the schema of [`ClientSettings`] and adds extra fields
/// to describe the workspace it applies to.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct WorkspaceSettings {
    #[serde(flatten)]
    settings: ClientSettings,
    workspace: Url,
}

/// This is the exact schema for initialization options sent in by the client
/// during initialization.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(untagged)]
enum InitializationOptions {
    #[serde(rename_all = "camelCase")]
    HasWorkspaces {
        global_settings: ClientSettings,
        #[serde(rename = "settings")]
        workspace_settings: Vec<WorkspaceSettings>,
    },
    GlobalOnly {
        #[serde(default)]
        settings: ClientSettings,
    },
}

/// Built from the initialization options provided by the client.
#[derive(Debug)]
pub(crate) struct AllSettings {
    pub(crate) global_settings: ClientSettings,
    /// If this is `None`, the client only passed in global settings.
    pub(crate) workspace_settings: Option<WorkspaceSettingsMap>,
}

impl AllSettings {
    /// Initializes the controller from the serialized initialization options.
    /// This fails if `options` are not valid initialization options.
    pub(crate) fn from_value(options: serde_json::Value) -> Self {
        Self::from_init_options(
            serde_json::from_value(options)
                .map_err(|err| {
                    tracing::error!("Failed to deserialize initialization options: {err}. Falling back to default client settings...");
                })
                .unwrap_or_default(),
        )
    }

    fn from_init_options(options: InitializationOptions) -> Self {
        let (global_settings, workspace_settings) = match options {
            InitializationOptions::GlobalOnly { settings } => (settings, None),
            InitializationOptions::HasWorkspaces {
                global_settings,
                workspace_settings,
            } => (global_settings, Some(workspace_settings)),
        };

        Self {
            global_settings,
            workspace_settings: workspace_settings.map(|workspace_settings| {
                workspace_settings
                    .into_iter()
                    .map(|settings| (settings.workspace, settings.settings))
                    .collect()
            }),
        }
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly {
            settings: ClientSettings::default(),
        }
    }
}

/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ResolvedClientSettings {
    disable_language_services: bool,
}

impl ResolvedClientSettings {
    pub(crate) fn is_language_services_disabled(&self) -> bool {
        self.disable_language_services
    }

    /// Resolves global settings only.
    pub(super) fn global(global_settings: &ClientSettings) -> Self {
        Self::new_impl(&[global_settings])
    }

    fn new_impl(all_settings: &[&ClientSettings]) -> Self {
        Self {
            disable_language_services: Self::resolve_or(
                all_settings,
                |settings| {
                    settings
                        .python
                        .as_ref()?
                        .ty
                        .as_ref()?
                        .disable_language_services
                },
                false,
            ),
        }
    }

    /// Attempts to resolve a setting using a list of available client settings as sources.
    /// Client settings that come earlier in the list take priority. `default` will be returned
    /// if none of the settings specify the requested setting.
    ///
    /// Use [`ResolvedClientSettings::resolve_optional`] if the setting should be optional instead
    /// of having a default value.
    fn resolve_or<T>(
        all_settings: &[&ClientSettings],
        get: impl Fn(&ClientSettings) -> Option<T>,
        default: T,
    ) -> T {
        Self::resolve_optional(all_settings, get).unwrap_or(default)
    }

    /// Attempts to resolve a setting using a list of available client settings as sources.
    /// Client settings that come earlier in the list take priority. This function is for fields
    /// that do not have a default value and should be left unset.
    ///
    /// Use [`ResolvedClientSettings::resolve_or`] for settings that should have default values.
    fn resolve_optional<T>(
        all_settings: &[&ClientSettings],
        get: impl FnMut(&ClientSettings) -> Option<T>,
    ) -> Option<T> {
        all_settings.iter().map(Deref::deref).find_map(get)
    }
}
