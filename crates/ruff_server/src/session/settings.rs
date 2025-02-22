use std::{ops::Deref, path::PathBuf, str::FromStr};

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use ruff_linter::{line_width::LineLength, RuleSelector};
use ruff_macros::OptionsMetadata;
use ruff_workspace::options_base::{OptionField, OptionsMetadata};

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
    disable_rule_comment_enable: bool,
    fix_violation_enable: bool,
    show_syntax_errors: bool,
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

#[derive(Debug, Deserialize, Default, OptionsMetadata)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub struct ClientSettings {
    /// Path to a `ruff.toml` or `pyproject.toml` file to use for configuration.
    ///
    /// By default, Ruff will discover configuration for each project from the filesystem,
    /// mirroring the behavior of the Ruff CLI.
    #[option(
        default = r#"null"#,
        value_type = "string",
        example = r#""~/path/to/ruff.toml""#
    )]
    configuration: Option<String>,

    /// The strategy to use when resolving settings across VS Code and the filesystem. By default,
    /// editor configuration is prioritized over `ruff.toml` and `pyproject.toml` files.
    ///
    /// * `"editorFirst"`: Editor settings take priority over configuration files present in the
    ///   workspace.
    /// * `"filesystemFirst"`: Configuration files present in the workspace takes priority over
    ///   editor settings.
    /// * `"editorOnly"`: Ignore configuration files entirely i.e., only use editor settings.
    #[option(
        default = r#""editorFirst""#,
        value_type = r#""editorFirst" | "filesystemFirst" | "editorOnly""#,
        example = r#""filesystemFirst""#
    )]
    configuration_preference: Option<ConfigurationPreference>,

    /// A list of file patterns to exclude from linting and formatting. See [the
    /// documentation](https://docs.astral.sh/ruff/settings/#exclude) for more details.
    #[option(
        default = r#"null"#,
        value_type = "string[]",
        example = r#"["**/tests/**"]"#
    )]
    exclude: Option<Vec<String>>,

    /// The line length to use for the linter and formatter.
    #[option(default = "null", value_type = "int", example = "100")]
    line_length: Option<LineLength>,

    /// Whether to register the server as capable of handling `source.fixAll` code actions.
    #[option(default = "true", value_type = "bool", example = "false")]
    fix_all: Option<bool>,

    /// Whether to register the server as capable of handling `source.organizeImports` code
    /// actions.
    #[option(default = "true", value_type = "bool", example = "false")]
    organize_imports: Option<bool>,

    /// _New in Ruff [v0.5.0](https://astral.sh/blog/ruff-v0.5.0#changes-to-e999-and-reporting-of-syntax-errors)_
    ///
    /// Whether to show syntax error diagnostics.
    ///
    /// This is useful when using Ruff with other language servers, allowing the user to refer
    /// to syntax errors from only one source.
    #[option(default = "true", value_type = "bool", example = "false")]
    show_syntax_errors: Option<bool>,

    #[option_group]
    lint: Option<LintOptions>,

    #[option_group]
    format: Option<FormatOptions>,

    #[option_group]
    code_action: Option<CodeActionOptions>,

    // These settings are only needed for tracing, and are only read from the global configuration.
    // These will not be in the resolved settings.
    #[serde(flatten)]
    pub(crate) tracing: TracingSettings,
}

impl ClientSettings {
    /// Update the preview flag for the linter and the formatter with the given value.
    pub(crate) fn set_preview(&mut self, preview: bool) {
        match self.lint.as_mut() {
            None => self.lint = Some(LintOptions::default().with_preview(preview)),
            Some(lint) => lint.set_preview(preview),
        }
        match self.format.as_mut() {
            None => self.format = Some(FormatOptions::default().with_preview(preview)),
            Some(format) => format.set_preview(preview),
        }
    }
}

#[derive(Debug, Deserialize, Default, OptionsMetadata)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TracingSettings {
    /// The log level to use for the server.
    #[option(
        default = r#""info""#,
        value_type = r#""error" | "warn" | "info" | "debug" | "trace""#,
        example = r#""debug""#
    )]
    pub(crate) log_level: Option<crate::logging::LogLevel>,

    /// Path to the log file to use for the server.
    ///
    /// If not set, logs will be written to stderr. Tildes and environment variables are expanded.
    #[option(
        default = r#"null"#,
        value_type = "string",
        example = r#""~/path/to/ruff.log""#
    )]
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

/// Settings specific to the Ruff linter.
#[derive(Debug, Default, Deserialize, OptionsMetadata)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct LintOptions {
    /// Whether to enable linting. Set to `false` to use Ruff exclusively as a formatter.
    #[option(default = "true", value_type = "bool", example = "false")]
    enable: Option<bool>,

    /// Whether to enable Ruff's preview mode when linting.
    #[option(default = "null", value_type = "bool", example = "true")]
    preview: Option<bool>,

    /// Rules to enable by default. See [the
    /// documentation](https://docs.astral.sh/ruff/settings/#lint_select).
    #[option(default = "null", value_type = "string[]", example = r#"["E", "F"]"#)]
    select: Option<Vec<String>>,

    /// Rules to enable in addition to those in [`lint.select`](#select).
    #[option(default = "null", value_type = "string[]", example = r#"["W"]"#)]
    extend_select: Option<Vec<String>>,

    /// Rules to disable by default. See [the
    /// documentation](https://docs.astral.sh/ruff/settings/#lint_ignore).
    #[option(default = "null", value_type = "string[]", example = r#"["E4", "E7"]"#)]
    ignore: Option<Vec<String>>,
}

impl LintOptions {
    fn with_preview(mut self, preview: bool) -> LintOptions {
        self.preview = Some(preview);
        self
    }

    fn set_preview(&mut self, preview: bool) {
        self.preview = Some(preview);
    }
}

/// Settings specific to the Ruff formatter.
#[derive(Debug, Default, Deserialize, OptionsMetadata)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct FormatOptions {
    /// Whether to enable Ruff's preview mode when formatting.
    #[option(default = "null", value_type = "bool", example = "true")]
    preview: Option<bool>,
}

impl FormatOptions {
    fn with_preview(mut self, preview: bool) -> FormatOptions {
        self.preview = Some(preview);
        self
    }

    fn set_preview(&mut self, preview: bool) {
        self.preview = Some(preview);
    }
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

impl OptionsMetadata for CodeActionOptions {
    fn record(visit: &mut dyn ruff_workspace::options_base::Visit) {
        visit.record_field(
            "disableRuleComment.enable",
            OptionField {
                doc: "Whether to display Quick Fix actions to disable rules via `noqa` suppression comments.",
                default: "true",
                value_type: "bool",
                scope: None,
                example: "false",
                deprecated: None,
            },
        );

        visit.record_field(
            "fixViolation.enable",
            OptionField {
                doc: "Whether to display Quick Fix actions to autofix violations.",
                default: "true",
                value_type: "bool",
                scope: None,
                example: "false",
                deprecated: None,
            },
        );
    }

    fn documentation() -> Option<&'static str> {
        Some("Enable or disable code actions provided by the server.")
    }
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
                    show_err_msg!("Ruff received invalid client settings - falling back to default client settings.");
                })
                .unwrap_or_default(),
        )
    }

    /// Update the preview flag for both the global and all workspace settings.
    pub(crate) fn set_preview(&mut self, preview: bool) {
        self.global_settings.set_preview(preview);
        if let Some(workspace_settings) = self.workspace_settings.as_mut() {
            for settings in workspace_settings.values_mut() {
                settings.set_preview(preview);
            }
        }
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
            show_syntax_errors: Self::resolve_or(
                all_settings,
                |settings| settings.show_syntax_errors,
                true,
            ),
            editor_settings: ResolvedEditorSettings {
                configuration: Self::resolve_optional(all_settings, |settings| {
                    settings
                        .configuration
                        .as_ref()
                        .and_then(|config_path| shellexpand::full(config_path).ok())
                        .map(|config_path| PathBuf::from(config_path.as_ref()))
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
                exclude: Self::resolve_optional(all_settings, |settings| settings.exclude.clone()),
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

    pub(crate) fn noqa_comments(&self) -> bool {
        self.disable_rule_comment_enable
    }

    pub(crate) fn fix_violation(&self) -> bool {
        self.fix_violation_enable
    }

    pub(crate) fn show_syntax_errors(&self) -> bool {
        self.show_syntax_errors
    }

    pub(crate) fn editor_settings(&self) -> &ResolvedEditorSettings {
        &self.editor_settings
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly {
            settings: ClientSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use serde::de::DeserializeOwned;

    #[cfg(not(windows))]
    use ruff_linter::registry::Linter;

    use super::*;

    #[cfg(not(windows))]
    const VS_CODE_INIT_OPTIONS_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/vs_code_initialization_options.json");
    const GLOBAL_ONLY_INIT_OPTIONS_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/global_only.json");
    const EMPTY_INIT_OPTIONS_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/empty.json");

    // This fixture contains multiple workspaces with empty initialization options. It only sets
    // the `cwd` and the `workspace` value.
    const EMPTY_MULTIPLE_WORKSPACE_INIT_OPTIONS_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/empty_multiple_workspace.json");

    fn deserialize_fixture<T: DeserializeOwned>(content: &str) -> T {
        serde_json::from_str(content).expect("test fixture JSON should deserialize")
    }

    #[cfg(not(windows))]
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
                show_syntax_errors: Some(
                    true,
                ),
                tracing: TracingSettings {
                    log_level: None,
                    log_file: None,
                },
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
                        show_syntax_errors: Some(
                            true,
                        ),
                        tracing: TracingSettings {
                            log_level: None,
                            log_file: None,
                        },
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
                        show_syntax_errors: Some(
                            true,
                        ),
                        tracing: TracingSettings {
                            log_level: None,
                            log_file: None,
                        },
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

    #[cfg(not(windows))]
    #[test]
    fn test_vs_code_workspace_settings_resolve() {
        let options = deserialize_fixture(VS_CODE_INIT_OPTIONS_FIXTURE);
        let AllSettings {
            global_settings,
            workspace_settings,
        } = AllSettings::from_init_options(options);
        let path =
            Url::from_str("file:///Users/test/projects/pandas").expect("path should be valid");
        let workspace_settings = workspace_settings.expect("workspace settings should exist");
        assert_eq!(
            ResolvedClientSettings::with_workspace(
                workspace_settings
                    .get(&path)
                    .expect("workspace setting should exist"),
                &global_settings
            ),
            ResolvedClientSettings {
                fix_all: true,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: false,
                fix_violation_enable: false,
                show_syntax_errors: true,
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
                },
            }
        );
        let path =
            Url::from_str("file:///Users/test/projects/scipy").expect("path should be valid");
        assert_eq!(
            ResolvedClientSettings::with_workspace(
                workspace_settings
                    .get(&path)
                    .expect("workspace setting should exist"),
                &global_settings
            ),
            ResolvedClientSettings {
                fix_all: true,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: true,
                fix_violation_enable: false,
                show_syntax_errors: true,
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
                },
            }
        );
    }

    #[test]
    fn test_global_only_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(GLOBAL_ONLY_INIT_OPTIONS_FIXTURE);

        assert_debug_snapshot!(options, @r#"
        GlobalOnly {
            settings: ClientSettings {
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
                show_syntax_errors: None,
                tracing: TracingSettings {
                    log_level: Some(
                        Warn,
                    ),
                    log_file: None,
                },
            },
        }
        "#);
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
                show_syntax_errors: true,
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
                },
            }
        );
    }

    #[test]
    fn test_empty_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(EMPTY_INIT_OPTIONS_FIXTURE);

        assert_eq!(options, InitializationOptions::default());
    }

    fn assert_preview_client_settings(settings: &ClientSettings, preview: bool) {
        assert_eq!(settings.lint.as_ref().unwrap().preview.unwrap(), preview);
        assert_eq!(settings.format.as_ref().unwrap().preview.unwrap(), preview);
    }

    fn assert_preview_all_settings(all_settings: &AllSettings, preview: bool) {
        assert_preview_client_settings(&all_settings.global_settings, preview);
        if let Some(workspace_settings) = all_settings.workspace_settings.as_ref() {
            for settings in workspace_settings.values() {
                assert_preview_client_settings(settings, preview);
            }
        }
    }

    #[test]
    fn test_preview_flag() {
        let options = deserialize_fixture(EMPTY_MULTIPLE_WORKSPACE_INIT_OPTIONS_FIXTURE);
        let mut all_settings = AllSettings::from_init_options(options);

        all_settings.set_preview(false);
        assert_preview_all_settings(&all_settings, false);

        all_settings.set_preview(true);
        assert_preview_all_settings(&all_settings, true);
    }
}
