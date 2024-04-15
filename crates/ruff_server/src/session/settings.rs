use std::ops::Deref;

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;

/// Maps a workspace URI to its associated client settings. Used during server initialization.
pub(crate) type WorkspaceSettingsMap = FxHashMap<Url, ClientSettings>;

/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Debug)]
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
}

/// This is a direct representation of the settings schema sent by the client.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientSettings {
    fix_all: Option<bool>,
    organize_imports: Option<bool>,
    lint: Option<Lint>,
    code_action: Option<CodeAction>,
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
        }
    }

    /// Attempts to resolve a setting using a list of available client settings as sources.
    /// Client settings that come earlier in the list take priority. `default` will be returned
    /// if none of the settings specify the requested setting.
    fn resolve_or<T>(
        all_settings: &[&ClientSettings],
        get: impl Fn(&ClientSettings) -> Option<T>,
        default: T,
    ) -> T {
        all_settings
            .iter()
            .map(Deref::deref)
            .find_map(get)
            .unwrap_or(default)
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
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly { settings: None }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
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
                fix_all: Some(
                    false,
                ),
                organize_imports: Some(
                    true,
                ),
                lint: Some(
                    Lint {
                        enable: Some(
                            true,
                        ),
                    },
                ),
                code_action: Some(
                    CodeAction {
                        disable_rule_comment: Some(
                            CodeActionSettings {
                                enable: Some(
                                    false,
                                ),
                            },
                        ),
                        fix_violation: Some(
                            CodeActionSettings {
                                enable: Some(
                                    false,
                                ),
                            },
                        ),
                    },
                ),
            },
            workspace_settings: [
                WorkspaceSettings {
                    settings: ClientSettings {
                        fix_all: Some(
                            true,
                        ),
                        organize_imports: Some(
                            true,
                        ),
                        lint: Some(
                            Lint {
                                enable: Some(
                                    true,
                                ),
                            },
                        ),
                        code_action: Some(
                            CodeAction {
                                disable_rule_comment: Some(
                                    CodeActionSettings {
                                        enable: Some(
                                            false,
                                        ),
                                    },
                                ),
                                fix_violation: Some(
                                    CodeActionSettings {
                                        enable: Some(
                                            false,
                                        ),
                                    },
                                ),
                            },
                        ),
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
                        fix_all: Some(
                            true,
                        ),
                        organize_imports: Some(
                            true,
                        ),
                        lint: Some(
                            Lint {
                                enable: Some(
                                    true,
                                ),
                            },
                        ),
                        code_action: Some(
                            CodeAction {
                                disable_rule_comment: Some(
                                    CodeActionSettings {
                                        enable: Some(
                                            true,
                                        ),
                                    },
                                ),
                                fix_violation: Some(
                                    CodeActionSettings {
                                        enable: Some(
                                            false,
                                        ),
                                    },
                                ),
                            },
                        ),
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
                    fix_all: Some(
                        false,
                    ),
                    organize_imports: None,
                    lint: Some(
                        Lint {
                            enable: None,
                        },
                    ),
                    code_action: Some(
                        CodeAction {
                            disable_rule_comment: Some(
                                CodeActionSettings {
                                    enable: Some(
                                        false,
                                    ),
                                },
                            ),
                            fix_violation: None,
                        },
                    ),
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
            }
        );
    }

    #[test]
    fn test_empty_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(EMPTY_INIT_OPTIONS_FIXTURE);

        assert_eq!(options, InitializationOptions::default());
    }
}
