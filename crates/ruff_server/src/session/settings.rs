use std::{ffi::OsString, ops::Deref, path::PathBuf, str::FromStr};

use lsp_types::Url;
use ruff_linter::{line_width::LineLength, RuleSelector};
use rustc_hash::FxHashMap;
use serde::Deserialize;

/// Maps a workspace URI to its associated client settings. Used during server initialization.
pub(crate) type WorkspaceSettingsMap = FxHashMap<Url, ClientSettings>;

/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ResolvedClientSettings {
    fix_all: bool,
    organize_imports: bool,
    lint_enable: bool,
    // TODO(jane): Remove once noqa auto-fix is implemented
    #[allow(dead_code)]
    disable_rule_comment_enable: bool,
    fix_violation_enable: bool,
    editor_settings: ResolvedEditorSettings,
}

/// Contains the resolved values of 'editor settings' - Ruff configuration for the linter/formatter that was passed in via
/// LSP client settings. These fields are optional because we don't want to override file-based linter/formatting settings
/// if these were un-set.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ResolvedEditorSettings {
    pub(super) configuration: Option<PathBuf>,
    pub(super) lint_preview: Option<bool>,
    pub(super) format_preview: Option<bool>,
    pub(super) select: Option<Vec<RuleSelector>>,
    pub(super) extend_select: Option<Vec<RuleSelector>>,
    pub(super) ignore: Option<Vec<RuleSelector>>,
    pub(super) exclude: Option<Vec<String>>,
    pub(super) line_length: Option<LineLength>,
    pub(super) configuration_preference: ConfigurationPreference,
}

/// Determines how multiple conflicting configurations should be resolved - in this
/// case, the configuration from the client settings and configuration from local
/// `.toml` files (aka 'workspace' configuration).
#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ConfigurationPreference {
    /// Configuration set in the editor takes priority over configuration set in `.toml` files.
    #[default]
    EditorFirst,
    /// Configuration set in `.toml` files takes priority over configuration set in the editor.
    FilesystemFirst,
    /// `.toml` files are ignored completely, and only the editor configuration is used.
    EditorOnly,
}

/// This is a direct representation of the settings schema sent by the client.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientSettings {
    configuration: Option<String>,
    fix_all: Option<bool>,
    organize_imports: Option<bool>,
    lint: Option<LintOptions>,
    format: Option<FormatOptions>,
    code_action: Option<CodeActionOptions>,
    exclude: Option<Vec<String>>,
    line_length: Option<LineLength>,
    configuration_preference: Option<ConfigurationPreference>,
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

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct LintOptions {
    enable: Option<bool>,
    preview: Option<bool>,
    select: Option<Vec<String>>,
    extend_select: Option<Vec<String>>,
    ignore: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct FormatOptions {
    preview: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct CodeActionOptions {
    disable_rule_comment: Option<CodeActionParameters>,
    fix_violation: Option<CodeActionParameters>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct CodeActionParameters {
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
        global_settings: ClientSettings,
        #[serde(rename = "settings")]
        workspace_settings: Vec<WorkspaceSettings>,
    },
    GlobalOnly {
        settings: Option<ClientSettings>,
    },
}

/// Built from the initialization options provided by the client.
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
                    show_err_msg!("Ruff received invalid client settings - falling back to default client settings.");
                })
                .unwrap_or_default(),
        )
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
            workspace_settings: workspace_settings.map(|workspace_settings| {
                workspace_settings
                    .into_iter()
                    .map(|settings| (settings.workspace, settings.settings))
                    .collect()
            }),
        }
    }
}

impl ResolvedClientSettings {
    /// Resolves a series of client settings, prioritizing workspace settings over global settings.
    /// Any fields not specified by either are set to their defaults.
    pub(super) fn with_workspace(
        workspace_settings: &ClientSettings,
        global_settings: &ClientSettings,
    ) -> Self {
        Self::new_impl(&[workspace_settings, global_settings])
    }

    /// Resolves global settings only.
    pub(super) fn global(global_settings: &ClientSettings) -> Self {
        Self::new_impl(&[global_settings])
    }

    fn new_impl(all_settings: &[&ClientSettings]) -> Self {
        Self {
            fix_all: Self::resolve_or(all_settings, |settings| settings.fix_all, true),
            organize_imports: Self::resolve_or(
                all_settings,
                |settings| settings.organize_imports,
                true,
            ),
            lint_enable: Self::resolve_or(
                all_settings,
                |settings| settings.lint.as_ref()?.enable,
                true,
            ),
            disable_rule_comment_enable: Self::resolve_or(
                all_settings,
                |settings| {
                    settings
                        .code_action
                        .as_ref()?
                        .disable_rule_comment
                        .as_ref()?
                        .enable
                },
                true,
            ),
            fix_violation_enable: Self::resolve_or(
                all_settings,
                |settings| {
                    settings
                        .code_action
                        .as_ref()?
                        .fix_violation
                        .as_ref()?
                        .enable
                },
                true,
            ),
            editor_settings: ResolvedEditorSettings {
                configuration: Self::resolve_optional(all_settings, |settings| {
                    settings
                        .configuration
                        .as_ref()
                        .map(|config_path| OsString::from(config_path.clone()).into())
                }),
                lint_preview: Self::resolve_optional(all_settings, |settings| {
                    settings.lint.as_ref()?.preview
                }),
                format_preview: Self::resolve_optional(all_settings, |settings| {
                    settings.format.as_ref()?.preview
                }),
                select: Self::resolve_optional(all_settings, |settings| {
                    settings
                        .lint
                        .as_ref()?
                        .select
                        .as_ref()?
                        .iter()
                        .map(|rule| RuleSelector::from_str(rule).ok())
                        .collect()
                }),
                extend_select: Self::resolve_optional(all_settings, |settings| {
                    settings
                        .lint
                        .as_ref()?
                        .extend_select
                        .as_ref()?
                        .iter()
                        .map(|rule| RuleSelector::from_str(rule).ok())
                        .collect()
                }),
                ignore: Self::resolve_optional(all_settings, |settings| {
                    settings
                        .lint
                        .as_ref()?
                        .ignore
                        .as_ref()?
                        .iter()
                        .map(|rule| RuleSelector::from_str(rule).ok())
                        .collect()
                }),
                exclude: Self::resolve_optional(all_settings, |settings| {
                    Some(settings.exclude.as_ref()?.clone())
                }),
                line_length: Self::resolve_optional(all_settings, |settings| settings.line_length),
                configuration_preference: Self::resolve_or(
                    all_settings,
                    |settings| settings.configuration_preference,
                    ConfigurationPreference::EditorFirst,
                ),
            },
        }
    }

    /// Attempts to resolve a setting using a list of available client settings as sources.
    /// Client settings that come earlier in the list take priority. This function is for fields
    /// that do not have a default value and should be left unset.
    /// Use [`ResolvedClientSettings::resolve_or`] for settings that should have default values.
    fn resolve_optional<T>(
        all_settings: &[&ClientSettings],
        get: impl Fn(&ClientSettings) -> Option<T>,
    ) -> Option<T> {
        all_settings.iter().map(Deref::deref).find_map(get)
    }

    /// Attempts to resolve a setting using a list of available client settings as sources.
    /// Client settings that come earlier in the list take priority. `default` will be returned
    /// if none of the settings specify the requested setting.
    /// Use [`ResolvedClientSettings::resolve_optional`] if the setting should be optional instead
    /// of having a default value.
    fn resolve_or<T>(
        all_settings: &[&ClientSettings],
        get: impl Fn(&ClientSettings) -> Option<T>,
        default: T,
    ) -> T {
        Self::resolve_optional(all_settings, get).unwrap_or(default)
    }
}

impl ResolvedClientSettings {
    pub(crate) fn fix_all(&self) -> bool {
        self.fix_all
    }

    pub(crate) fn organize_imports(&self) -> bool {
        self.organize_imports
    }

    pub(crate) fn lint(&self) -> bool {
        self.lint_enable
    }

    pub(crate) fn fix_violation(&self) -> bool {
        self.fix_violation_enable
    }

    pub(crate) fn editor_settings(&self) -> &ResolvedEditorSettings {
        &self.editor_settings
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly { settings: None }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use ruff_linter::registry::Linter;
    use serde::de::DeserializeOwned;

    use super::*;

    const VS_CODE_INIT_OPTIONS_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/vs_code_initialization_options.json");
    const GLOBAL_ONLY_INIT_OPTIONS_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/global_only.json");
    const EMPTY_INIT_OPTIONS_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/empty.json");

    fn deserialize_fixture<T: DeserializeOwned>(content: &str) -> T {
        serde_json::from_str(content).expect("test fixture JSON should deserialize")
    }

    #[test]
    fn test_vs_code_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(VS_CODE_INIT_OPTIONS_FIXTURE);

        assert_debug_snapshot!(options, @r###"
        HasWorkspaces {
            global_settings: ClientSettings {
                configuration: None,
                fix_all: Some(
                    false,
                ),
                organize_imports: Some(
                    true,
                ),
                lint: Some(
                    LintOptions {
                        enable: Some(
                            true,
                        ),
                        preview: Some(
                            true,
                        ),
                        select: Some(
                            [
                                "F",
                                "I",
                            ],
                        ),
                        extend_select: None,
                        ignore: None,
                    },
                ),
                format: Some(
                    FormatOptions {
                        preview: None,
                    },
                ),
                code_action: Some(
                    CodeActionOptions {
                        disable_rule_comment: Some(
                            CodeActionParameters {
                                enable: Some(
                                    false,
                                ),
                            },
                        ),
                        fix_violation: Some(
                            CodeActionParameters {
                                enable: Some(
                                    false,
                                ),
                            },
                        ),
                    },
                ),
                exclude: None,
                line_length: None,
                configuration_preference: None,
            },
            workspace_settings: [
                WorkspaceSettings {
                    settings: ClientSettings {
                        configuration: None,
                        fix_all: Some(
                            true,
                        ),
                        organize_imports: Some(
                            true,
                        ),
                        lint: Some(
                            LintOptions {
                                enable: Some(
                                    true,
                                ),
                                preview: None,
                                select: None,
                                extend_select: None,
                                ignore: None,
                            },
                        ),
                        format: Some(
                            FormatOptions {
                                preview: None,
                            },
                        ),
                        code_action: Some(
                            CodeActionOptions {
                                disable_rule_comment: Some(
                                    CodeActionParameters {
                                        enable: Some(
                                            false,
                                        ),
                                    },
                                ),
                                fix_violation: Some(
                                    CodeActionParameters {
                                        enable: Some(
                                            false,
                                        ),
                                    },
                                ),
                            },
                        ),
                        exclude: None,
                        line_length: None,
                        configuration_preference: None,
                    },
                    workspace: Url {
                        scheme: "file",
                        cannot_be_a_base: false,
                        username: "",
                        password: None,
                        host: None,
                        port: None,
                        path: "/Users/test/projects/pandas",
                        query: None,
                        fragment: None,
                    },
                },
                WorkspaceSettings {
                    settings: ClientSettings {
                        configuration: None,
                        fix_all: Some(
                            true,
                        ),
                        organize_imports: Some(
                            true,
                        ),
                        lint: Some(
                            LintOptions {
                                enable: Some(
                                    true,
                                ),
                                preview: Some(
                                    false,
                                ),
                                select: None,
                                extend_select: None,
                                ignore: None,
                            },
                        ),
                        format: Some(
                            FormatOptions {
                                preview: None,
                            },
                        ),
                        code_action: Some(
                            CodeActionOptions {
                                disable_rule_comment: Some(
                                    CodeActionParameters {
                                        enable: Some(
                                            true,
                                        ),
                                    },
                                ),
                                fix_violation: Some(
                                    CodeActionParameters {
                                        enable: Some(
                                            false,
                                        ),
                                    },
                                ),
                            },
                        ),
                        exclude: None,
                        line_length: None,
                        configuration_preference: None,
                    },
                    workspace: Url {
                        scheme: "file",
                        cannot_be_a_base: false,
                        username: "",
                        password: None,
                        host: None,
                        port: None,
                        path: "/Users/test/projects/scipy",
                        query: None,
                        fragment: None,
                    },
                },
            ],
        }
        "###);
    }

    #[test]
    fn test_vs_code_workspace_settings_resolve() {
        let options = deserialize_fixture(VS_CODE_INIT_OPTIONS_FIXTURE);
        let AllSettings {
            global_settings,
            workspace_settings,
        } = AllSettings::from_init_options(options);
        let url = Url::parse("file:///Users/test/projects/pandas").expect("url should parse");
        let workspace_settings = workspace_settings.expect("workspace settings should exist");
        assert_eq!(
            ResolvedClientSettings::with_workspace(
                workspace_settings
                    .get(&url)
                    .expect("workspace setting should exist"),
                &global_settings
            ),
            ResolvedClientSettings {
                fix_all: true,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: false,
                fix_violation_enable: false,
                editor_settings: ResolvedEditorSettings {
                    configuration: None,
                    lint_preview: Some(true),
                    format_preview: None,
                    select: Some(vec![
                        RuleSelector::Linter(Linter::Pyflakes),
                        RuleSelector::Linter(Linter::Isort)
                    ]),
                    extend_select: None,
                    ignore: None,
                    exclude: None,
                    line_length: None,
                    configuration_preference: ConfigurationPreference::default(),
                }
            }
        );
        let url = Url::parse("file:///Users/test/projects/scipy").expect("url should parse");
        assert_eq!(
            ResolvedClientSettings::with_workspace(
                workspace_settings
                    .get(&url)
                    .expect("workspace setting should exist"),
                &global_settings
            ),
            ResolvedClientSettings {
                fix_all: true,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: true,
                fix_violation_enable: false,
                editor_settings: ResolvedEditorSettings {
                    configuration: None,
                    lint_preview: Some(false),
                    format_preview: None,
                    select: Some(vec![
                        RuleSelector::Linter(Linter::Pyflakes),
                        RuleSelector::Linter(Linter::Isort)
                    ]),
                    extend_select: None,
                    ignore: None,
                    exclude: None,
                    line_length: None,
                    configuration_preference: ConfigurationPreference::EditorFirst,
                }
            }
        );
    }

    #[test]
    fn test_global_only_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(GLOBAL_ONLY_INIT_OPTIONS_FIXTURE);

        assert_debug_snapshot!(options, @r###"
        GlobalOnly {
            settings: Some(
                ClientSettings {
                    configuration: None,
                    fix_all: Some(
                        false,
                    ),
                    organize_imports: None,
                    lint: Some(
                        LintOptions {
                            enable: None,
                            preview: None,
                            select: None,
                            extend_select: None,
                            ignore: Some(
                                [
                                    "RUF001",
                                ],
                            ),
                        },
                    ),
                    format: None,
                    code_action: Some(
                        CodeActionOptions {
                            disable_rule_comment: Some(
                                CodeActionParameters {
                                    enable: Some(
                                        false,
                                    ),
                                },
                            ),
                            fix_violation: None,
                        },
                    ),
                    exclude: Some(
                        [
                            "third_party",
                        ],
                    ),
                    line_length: Some(
                        LineLength(
                            80,
                        ),
                    ),
                    configuration_preference: None,
                },
            ),
        }
        "###);
    }

    #[test]
    fn test_global_only_resolves_correctly() {
        let options = deserialize_fixture(GLOBAL_ONLY_INIT_OPTIONS_FIXTURE);

        let AllSettings {
            global_settings, ..
        } = AllSettings::from_init_options(options);
        assert_eq!(
            ResolvedClientSettings::global(&global_settings),
            ResolvedClientSettings {
                fix_all: false,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: false,
                fix_violation_enable: true,
                editor_settings: ResolvedEditorSettings {
                    configuration: None,
                    lint_preview: None,
                    format_preview: None,
                    select: None,
                    extend_select: None,
                    ignore: Some(vec![RuleSelector::from_str("RUF001").unwrap()]),
                    exclude: Some(vec!["third_party".into()]),
                    line_length: Some(LineLength::try_from(80).unwrap()),
                    configuration_preference: ConfigurationPreference::EditorFirst,
                }
            }
        );
    }

    #[test]
    fn test_empty_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(EMPTY_INIT_OPTIONS_FIXTURE);

        assert_eq!(options, InitializationOptions::default());
    }
}
