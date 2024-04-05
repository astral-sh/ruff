use std::ops::Deref;

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;

#[cfg(test)]
mod tests;

pub(crate) type WorkspaceSettingsMap = FxHashMap<Url, UserSettings>;

/// Built from the initialization options provided by the client.
pub(crate) struct AllSettings {
    pub(crate) global_settings: UserSettings,
    pub(crate) workspace_settings: WorkspaceSettingsMap,
}

/// Resolved user settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[allow(dead_code, clippy::struct_excessive_bools)]
pub(crate) struct ResolvedUserSettings {
    fix_all: bool,
    organize_imports: bool,
    lint_enable: bool,
    disable_rule_comment_enable: bool,
    fix_violation_enable: bool,
}

/// This is a direct representation of the user settings schema sent by the client.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserSettings {
    fix_all: Option<bool>,
    organize_imports: Option<bool>,
    lint: Option<Lint>,
    code_action: Option<CodeAction>,
}

/// This is a direct representation of the workspace settings schema,
/// which inherits the schema of [`UserSettings`] and adds extra fields
/// to describe the workspace it applies to.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct WorkspaceSettings {
    #[serde(flatten)]
    user_settings: UserSettings,
    workspace: Url,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct Lint {
    enable: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct CodeAction {
    disable_rule_comment: Option<CodeActionSettings>,
    fix_violation: Option<CodeActionSettings>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct CodeActionSettings {
    enable: Option<bool>,
}

/// This is the exact schema for initialization options sent in by the client
/// during initialization.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(untagged)]
enum InitializationOptions {
    #[serde(rename_all = "camelCase")]
    HasWorkspaces {
        global_settings: UserSettings,
        #[serde(rename = "settings")]
        workspace_settings: Vec<WorkspaceSettings>,
    },
    GlobalOnly {
        settings: Option<UserSettings>,
    },
}

impl AllSettings {
    /// Initializes the controller from the serialized initialization options.
    /// This fails if `options` are not valid initialization options.
    pub(crate) fn from_value(options: serde_json::Value) -> crate::Result<Self> {
        let options = serde_json::from_value(options).map_err(|err| {
            anyhow::anyhow!("Failed to deserialize initialization options: {err}")
        })?;
        Ok(Self::from_init_options(options))
    }

    fn from_init_options(options: InitializationOptions) -> Self {
        let (global_settings, workspace_settings) = match options {
            InitializationOptions::GlobalOnly { settings } => (settings.unwrap_or_default(), None),
            InitializationOptions::HasWorkspaces {
                global_settings,
                workspace_settings,
            } => (global_settings, Some(workspace_settings)),
        };

        Self {
            global_settings,
            workspace_settings: workspace_settings
                .into_iter()
                .flatten()
                .map(|workspace_settings| {
                    (
                        workspace_settings.workspace,
                        workspace_settings.user_settings,
                    )
                })
                .collect(),
        }
    }
}

impl ResolvedUserSettings {
    /// Resolves a series of user settings, prioritizing workspace settings over global settings.
    /// Any fields not specified by either are set to their defaults.
    pub(super) fn with_workspace(
        workspace_settings: &UserSettings,
        global_settings: &UserSettings,
    ) -> Self {
        Self::new_impl(&[workspace_settings, global_settings])
    }

    /// Resolves global settings only.
    pub(super) fn global_only(global_settings: &UserSettings) -> Self {
        Self::new_impl(&[global_settings])
    }

    fn new_impl(all_settings: &[&UserSettings]) -> Self {
        Self {
            fix_all: Self::resolve_or(all_settings, |settings| settings.fix_all, true),
            organize_imports: Self::resolve_or(
                all_settings,
                |settings| settings.organize_imports,
                true,
            ),
            lint_enable: Self::resolve_or(
                all_settings,
                |settings| settings.lint.as_ref().and_then(|lint| lint.enable),
                true,
            ),
            disable_rule_comment_enable: Self::resolve_or(
                all_settings,
                |settings| {
                    settings
                        .code_action
                        .as_ref()
                        .and_then(|code_action| code_action.disable_rule_comment.as_ref())
                        .and_then(|disable_rule_comment| disable_rule_comment.enable)
                },
                true,
            ),
            fix_violation_enable: Self::resolve_or(
                all_settings,
                |settings| {
                    settings
                        .code_action
                        .as_ref()
                        .and_then(|code_action| code_action.fix_violation.as_ref())
                        .and_then(|fix_violation| fix_violation.enable)
                },
                true,
            ),
        }
    }

    fn resolve_or<T>(
        all_settings: &[&UserSettings],
        get: impl Fn(&UserSettings) -> Option<T>,
        default: T,
    ) -> T {
        all_settings
            .iter()
            .map(Deref::deref)
            .find_map(get)
            .unwrap_or(default)
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly { settings: None }
    }
}
