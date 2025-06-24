use std::path::PathBuf;

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::logging::LogLevel;
use crate::session::settings::ClientSettings;

pub(crate) type WorkspaceOptionsMap = FxHashMap<Url, ClientOptions>;

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct GlobalOptions {
    #[serde(flatten)]
    client: ClientOptions,

    // These settings are only needed for tracing, and are only read from the global configuration.
    // These will not be in the resolved settings.
    #[serde(flatten)]
    pub(crate) tracing: TracingOptions,
}

impl GlobalOptions {
    pub(crate) fn into_settings(self) -> ClientSettings {
        ClientSettings {
            disable_language_services: self
                .client
                .python
                .and_then(|python| python.ty)
                .and_then(|ty| ty.disable_language_services)
                .unwrap_or_default(),
        }
    }
}

/// This is a direct representation of the workspace settings schema, which inherits the schema of
/// [`ClientOptions`] and adds extra fields to describe the workspace it applies to.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct WorkspaceOptions {
    #[serde(flatten)]
    options: ClientOptions,
    workspace: Url,
}

/// This is a direct representation of the settings schema sent by the client.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientOptions {
    /// Settings under the `python.*` namespace in VS Code that are useful for the ty language
    /// server.
    python: Option<Python>,
}

// TODO(dhruvmanila): We need to mirror the "python.*" namespace on the server side but ideally it
// would be useful to instead use `workspace/configuration` instead. This would be then used to get
// all settings and not just the ones in "python.*".

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

/// This is a direct representation of the settings schema sent by the client.
/// Settings needed to initialize tracing. These will only be read from the global configuration.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TracingOptions {
    pub(crate) log_level: Option<LogLevel>,

    /// Path to the log file - tildes and environment variables are supported.
    pub(crate) log_file: Option<PathBuf>,
}

/// This is the exact schema for initialization options sent in by the client during
/// initialization.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(untagged)]
enum InitializationOptions {
    #[serde(rename_all = "camelCase")]
    HasWorkspaces {
        #[serde(rename = "globalSettings")]
        global: GlobalOptions,
        #[serde(rename = "settings")]
        workspace: Vec<WorkspaceOptions>,
    },
    GlobalOnly {
        #[serde(default)]
        settings: GlobalOptions,
    },
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly {
            settings: GlobalOptions::default(),
        }
    }
}

/// Built from the initialization options provided by the client.
#[derive(Debug)]
pub(crate) struct AllOptions {
    pub(crate) global: GlobalOptions,
    /// If this is `None`, the client only passed in global settings.
    pub(crate) workspace: Option<WorkspaceOptionsMap>,
}

impl AllOptions {
    /// Initializes the controller from the serialized initialization options. This fails if
    /// `options` are not valid initialization options.
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
        let (global_options, workspace_options) = match options {
            InitializationOptions::GlobalOnly { settings: options } => (options, None),
            InitializationOptions::HasWorkspaces {
                global: global_options,
                workspace: workspace_options,
            } => (global_options, Some(workspace_options)),
        };

        Self {
            global: global_options,
            workspace: workspace_options.map(|workspace_options| {
                workspace_options
                    .into_iter()
                    .map(|workspace_options| {
                        (workspace_options.workspace, workspace_options.options)
                    })
                    .collect()
            }),
        }
    }
}
