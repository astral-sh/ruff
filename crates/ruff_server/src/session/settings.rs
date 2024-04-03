use std::path::Path;

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;

#[cfg(test)]
mod tests;

/// Built from the initialization options sent by the client.
/// If workspace settings were specified, `workspace_settings` will be populated.
/// Otherwise, it will be empty.
#[derive(Debug)]
pub(crate) struct SettingsController {
    global_settings: UserSettings,
    workspace_settings: FxHashMap<Url, UserSettings>,
}

/// Resolved user settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[allow(dead_code)]
pub(crate) struct ResolvedUserSettings {
    fix_all: FixAll,
    organize_imports: OrganizeImports,
    lint_enable: LintEnable,
    run: RunWhen,
    disable_rule_comment_enable: CodeActionEnable,
    fix_violation_enable: CodeActionEnable,
    log_level: LogLevel,
}

/// This is a direct representation of the user settings schema sent by the client.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct UserSettings {
    fix_all: Option<FixAll>,
    organize_imports: Option<OrganizeImports>,
    lint: Option<Lint>,
    code_action: Option<CodeAction>,
    log_level: Option<LogLevel>,
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

#[derive(Clone, Copy, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(transparent)]
#[repr(transparent)]
struct FixAll(bool);

#[derive(Clone, Copy, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(transparent)]
#[repr(transparent)]
struct OrganizeImports(bool);

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct Lint {
    enable: Option<LintEnable>,
    run: Option<RunWhen>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(transparent)]
#[repr(transparent)]
struct LintEnable(bool);

#[derive(Clone, Copy, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
enum RunWhen {
    OnType,
    OnSave,
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
    enable: Option<CodeActionEnable>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(transparent)]
#[repr(transparent)]
struct CodeActionEnable(bool);

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    #[default]
    Error,
    Warn,
    Info,
    Debug,
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

impl SettingsController {
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

    /// Resolves user settings for a workspace.
    /// `path` must be an exact path to the workspace - passing in a path
    /// to a document will not work here.
    pub(super) fn settings_for_workspace(&self, path: &Path) -> ResolvedUserSettings {
        let Ok(url) = Url::from_file_path(path) else {
            tracing::error!(
                "Workspace path {} could not be turned into a URI",
                path.display()
            );
            return self.global_settings();
        };
        if let Some(workspace_settings) = self.workspace_settings.get(&url) {
            ResolvedUserSettings::resolve(&[workspace_settings, &self.global_settings])
        } else {
            // If workspace settings are empty, this likely means that the initialization options
            // were `GlobalOnly`.
            // This is what most LSP clients will use, so we don't want them to barrage them
            // with misleading warnings.
            if !self.workspace_settings.is_empty() {
                tracing::warn!(
                    "No workspace-specific settings were found for workspace path {url}"
                );
            }
            self.global_settings()
        }
    }

    /// Resolves the global user settings, ignoring any workspace-specific
    /// overrides.
    pub(super) fn global_settings(&self) -> ResolvedUserSettings {
        ResolvedUserSettings::resolve(&[&self.global_settings])
    }
}

impl ResolvedUserSettings {
    /// Resolves a series of user settings in order of priority,
    /// where the highest-priority settings come first.
    /// Any fields not specified by any of these user settings are set to their defaults.
    fn resolve(all_settings: &[&UserSettings]) -> Self {
        Self {
            fix_all: Self::_resolve(all_settings, |settings| settings.fix_all),
            organize_imports: Self::_resolve(all_settings, |settings| settings.organize_imports),
            lint_enable: Self::_resolve(all_settings, |settings| {
                settings.lint.as_ref().and_then(|lint| lint.enable)
            }),
            run: Self::_resolve(all_settings, |settings| {
                settings.lint.as_ref().and_then(|lint| lint.run)
            }),
            disable_rule_comment_enable: Self::_resolve(all_settings, |settings| {
                settings
                    .code_action
                    .as_ref()
                    .and_then(|code_action| code_action.disable_rule_comment.as_ref())
                    .and_then(|disable_rule_comment| disable_rule_comment.enable)
            }),
            fix_violation_enable: Self::_resolve(all_settings, |settings| {
                settings
                    .code_action
                    .as_ref()
                    .and_then(|code_action| code_action.fix_violation.as_ref())
                    .and_then(|fix_violation| fix_violation.enable)
            }),
            log_level: Self::_resolve(all_settings, |settings| settings.log_level),
        }
    }

    fn _resolve<T: Default>(
        all_settings: &[&UserSettings],
        get: impl Fn(&&UserSettings) -> Option<T>,
    ) -> T {
        all_settings.iter().find_map(get).unwrap_or_default()
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly { settings: None }
    }
}

impl Default for FixAll {
    fn default() -> Self {
        Self(true)
    }
}

impl Default for OrganizeImports {
    fn default() -> Self {
        Self(true)
    }
}

impl Default for LintEnable {
    fn default() -> Self {
        Self(true)
    }
}

impl Default for RunWhen {
    fn default() -> Self {
        Self::OnType
    }
}

impl Default for CodeActionEnable {
    fn default() -> Self {
        Self(true)
    }
}
