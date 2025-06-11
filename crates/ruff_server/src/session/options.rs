use std::{path::PathBuf, str::FromStr as _};

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use serde_json::{Map, Value};

use ruff_linter::{RuleSelector, line_width::LineLength, rule_selector::ParseError};

use crate::session::{
    Client,
    settings::{ClientSettings, EditorSettings, GlobalClientSettings, ResolvedConfiguration},
};

pub(crate) type WorkspaceOptionsMap = FxHashMap<Url, ClientOptions>;

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

/// A direct representation of of `configuration` schema within the client settings.
#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(untagged)]
pub(super) enum ClientConfiguration {
    /// A path to a configuration file.
    String(String),
    /// An object containing the configuration options.
    Object(Map<String, Value>),
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub struct GlobalOptions {
    #[serde(flatten)]
    client: ClientOptions,

    // These settings are only needed for tracing, and are only read from the global configuration.
    // These will not be in the resolved settings.
    #[serde(flatten)]
    pub(crate) tracing: TracingOptions,
}

impl GlobalOptions {
    pub(crate) fn set_preview(&mut self, preview: bool) {
        self.client.set_preview(preview);
    }

    #[cfg(test)]
    pub(crate) fn client(&self) -> &ClientOptions {
        &self.client
    }

    pub fn into_settings(self, client: Client) -> GlobalClientSettings {
        GlobalClientSettings {
            options: self.client,
            settings: std::cell::OnceCell::default(),
            client,
        }
    }
}

/// This is a direct representation of the settings schema sent by the client.
#[derive(Clone, Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub struct ClientOptions {
    configuration: Option<ClientConfiguration>,
    fix_all: Option<bool>,
    organize_imports: Option<bool>,
    lint: Option<LintOptions>,
    format: Option<FormatOptions>,
    code_action: Option<CodeActionOptions>,
    exclude: Option<Vec<String>>,
    line_length: Option<LineLength>,
    configuration_preference: Option<ConfigurationPreference>,

    /// If `true` or [`None`], show syntax errors as diagnostics.
    ///
    /// This is useful when using Ruff with other language servers, allowing the user to refer
    /// to syntax errors from only one source.
    show_syntax_errors: Option<bool>,
}

impl ClientOptions {
    /// Resolves the options.
    ///
    /// Returns `Ok` if all options are valid. Otherwise, returns `Err` with the partially resolved settings
    /// (ignoring any invalid settings). Error messages about the invalid settings are logged with tracing.
    #[expect(
        clippy::result_large_err,
        reason = "The error is as large as the Ok variant"
    )]
    pub(crate) fn into_settings(self) -> Result<ClientSettings, ClientSettings> {
        let code_action = self.code_action.unwrap_or_default();
        let lint = self.lint.unwrap_or_default();
        let format = self.format.unwrap_or_default();
        let mut contains_invalid_settings = false;

        let configuration = self.configuration.and_then(|configuration| {
            match ResolvedConfiguration::try_from(configuration) {
                Ok(configuration) => Some(configuration),
                Err(err) => {
                    tracing::error!("Failed to load settings from `configuration`: {err}");
                    contains_invalid_settings = true;
                    None
                }
            }
        });

        let editor_settings = EditorSettings {
            configuration,
            lint_preview: lint.preview,
            format_preview: format.preview,
            select: lint.select.and_then(|select| {
                Self::resolve_rules(
                    &select,
                    RuleSelectorKey::Select,
                    &mut contains_invalid_settings,
                )
            }),
            extend_select: lint.extend_select.and_then(|select| {
                Self::resolve_rules(
                    &select,
                    RuleSelectorKey::ExtendSelect,
                    &mut contains_invalid_settings,
                )
            }),
            ignore: lint.ignore.and_then(|ignore| {
                Self::resolve_rules(
                    &ignore,
                    RuleSelectorKey::Ignore,
                    &mut contains_invalid_settings,
                )
            }),
            exclude: self.exclude.clone(),
            line_length: self.line_length,
            configuration_preference: self.configuration_preference.unwrap_or_default(),
        };

        let resolved = ClientSettings {
            editor_settings,
            fix_all: self.fix_all.unwrap_or(true),
            organize_imports: self.organize_imports.unwrap_or(true),
            lint_enable: lint.enable.unwrap_or(true),
            disable_rule_comment_enable: code_action
                .disable_rule_comment
                .and_then(|disable| disable.enable)
                .unwrap_or(true),
            fix_violation_enable: code_action
                .fix_violation
                .and_then(|fix| fix.enable)
                .unwrap_or(true),

            show_syntax_errors: self.show_syntax_errors.unwrap_or(true),
        };

        if contains_invalid_settings {
            Err(resolved)
        } else {
            Ok(resolved)
        }
    }

    fn resolve_rules(
        rules: &[String],
        key: RuleSelectorKey,
        contains_invalid_settings: &mut bool,
    ) -> Option<Vec<RuleSelector>> {
        let (mut known, mut unknown) = (vec![], vec![]);
        for rule in rules {
            match RuleSelector::from_str(rule) {
                Ok(selector) => known.push(selector),
                Err(ParseError::Unknown(_)) => unknown.push(rule),
            }
        }
        if !unknown.is_empty() {
            *contains_invalid_settings = true;
            tracing::error!("Unknown rule selectors found in `{key}`: {unknown:?}");
        }
        if known.is_empty() { None } else { Some(known) }
    }

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

impl Combine for ClientOptions {
    fn combine_with(&mut self, other: Self) {
        self.configuration.combine_with(other.configuration);
        self.fix_all.combine_with(other.fix_all);
        self.organize_imports.combine_with(other.organize_imports);
        self.lint.combine_with(other.lint);
        self.format.combine_with(other.format);
        self.code_action.combine_with(other.code_action);
        self.exclude.combine_with(other.exclude);
        self.line_length.combine_with(other.line_length);
        self.configuration_preference
            .combine_with(other.configuration_preference);
        self.show_syntax_errors
            .combine_with(other.show_syntax_errors);
    }
}

/// Settings needed to initialize tracing. These will only be
/// read from the global configuration.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TracingOptions {
    pub(crate) log_level: Option<crate::logging::LogLevel>,
    /// Path to the log file - tildes and environment variables are supported.
    pub(crate) log_file: Option<PathBuf>,
}

/// This is a direct representation of the workspace settings schema,
/// which inherits the schema of [`ClientOptions`] and adds extra fields
/// to describe the workspace it applies to.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct WorkspaceOptions {
    #[serde(flatten)]
    options: ClientOptions,
    workspace: Url,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct LintOptions {
    enable: Option<bool>,
    preview: Option<bool>,
    select: Option<Vec<String>>,
    extend_select: Option<Vec<String>>,
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

impl Combine for LintOptions {
    fn combine_with(&mut self, other: Self) {
        self.enable.combine_with(other.enable);
        self.preview.combine_with(other.preview);
        self.select.combine_with(other.select);
        self.extend_select.combine_with(other.extend_select);
        self.ignore.combine_with(other.ignore);
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct FormatOptions {
    preview: Option<bool>,
}

impl Combine for FormatOptions {
    fn combine_with(&mut self, other: Self) {
        self.preview.combine_with(other.preview);
    }
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

#[derive(Clone, Debug, Default, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct CodeActionOptions {
    disable_rule_comment: Option<CodeActionParameters>,
    fix_violation: Option<CodeActionParameters>,
}

impl Combine for CodeActionOptions {
    fn combine_with(&mut self, other: Self) {
        self.disable_rule_comment
            .combine_with(other.disable_rule_comment);
        self.fix_violation.combine_with(other.fix_violation);
    }
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct CodeActionParameters {
    enable: Option<bool>,
}

impl Combine for CodeActionParameters {
    fn combine_with(&mut self, other: Self) {
        self.enable.combine_with(other.enable);
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
    /// Initializes the controller from the serialized initialization options.
    /// This fails if `options` are not valid initialization options.
    pub(crate) fn from_value(options: serde_json::Value, client: &Client) -> Self {
        Self::from_init_options(
            serde_json::from_value(options)
                .map_err(|err| {
                    tracing::error!("Failed to deserialize initialization options: {err}. Falling back to default client settings...");
                    client.show_error_message("Ruff received invalid client settings - falling back to default client settings.");
                })
                .unwrap_or_default(),
        )
    }

    /// Update the preview flag for both the global and all workspace settings.
    pub(crate) fn set_preview(&mut self, preview: bool) {
        self.global.set_preview(preview);
        if let Some(workspace_options) = self.workspace.as_mut() {
            for options in workspace_options.values_mut() {
                options.set_preview(preview);
            }
        }
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

#[derive(Copy, Clone)]
enum RuleSelectorKey {
    Select,
    ExtendSelect,
    Ignore,
}

impl std::fmt::Display for RuleSelectorKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleSelectorKey::Select => f.write_str("lint.select"),
            RuleSelectorKey::ExtendSelect => f.write_str("lint.extendSelect"),
            RuleSelectorKey::Ignore => f.write_str("lint.ignore"),
        }
    }
}

pub(crate) trait Combine {
    #[must_use]
    fn combine(mut self, other: Self) -> Self
    where
        Self: Sized,
    {
        self.combine_with(other);
        self
    }

    fn combine_with(&mut self, other: Self);
}

impl<T> Combine for Option<T>
where
    T: Combine,
{
    fn combine(self, other: Self) -> Self
    where
        Self: Sized,
    {
        match (self, other) {
            (Some(a), Some(b)) => Some(a.combine(b)),
            (None, Some(b)) => Some(b),
            (a, _) => a,
        }
    }

    fn combine_with(&mut self, other: Self) {
        match (self, other) {
            (Some(a), Some(b)) => {
                a.combine_with(b);
            }
            (a @ None, Some(b)) => {
                *a = Some(b);
            }
            _ => {}
        }
    }
}

impl<T> Combine for Vec<T> {
    fn combine_with(&mut self, _other: Self) {
        // No-op, use own elements
    }
}

/// Implements [`Combine`] for a value that always returns `self` when combined with another value.
macro_rules! impl_noop_combine {
    ($name:ident) => {
        impl Combine for $name {
            #[inline(always)]
            fn combine_with(&mut self, _other: Self) {}

            #[inline(always)]
            fn combine(self, _other: Self) -> Self {
                self
            }
        }
    };
}

// std types
impl_noop_combine!(bool);
impl_noop_combine!(usize);
impl_noop_combine!(u8);
impl_noop_combine!(u16);
impl_noop_combine!(u32);
impl_noop_combine!(u64);
impl_noop_combine!(u128);
impl_noop_combine!(isize);
impl_noop_combine!(i8);
impl_noop_combine!(i16);
impl_noop_combine!(i32);
impl_noop_combine!(i64);
impl_noop_combine!(i128);
impl_noop_combine!(String);

// Custom types
impl_noop_combine!(ConfigurationPreference);
impl_noop_combine!(ClientConfiguration);
impl_noop_combine!(LineLength);

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use ruff_python_formatter::QuoteStyle;
    use ruff_workspace::options::{
        FormatOptions as RuffFormatOptions, LintCommonOptions, LintOptions, Options,
    };
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

    const INLINE_CONFIGURATION_FIXTURE: &str =
        include_str!("../../resources/test/fixtures/settings/inline_configuration.json");

    fn deserialize_fixture<T: DeserializeOwned>(content: &str) -> T {
        serde_json::from_str(content).expect("test fixture JSON should deserialize")
    }

    #[cfg(not(windows))]
    #[test]
    fn test_vs_code_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(VS_CODE_INIT_OPTIONS_FIXTURE);

        assert_debug_snapshot!(options, @r#"
        HasWorkspaces {
            global: GlobalOptions {
                client: ClientOptions {
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
                },
                tracing: TracingOptions {
                    log_level: None,
                    log_file: None,
                },
            },
            workspace: [
                WorkspaceOptions {
                    options: ClientOptions {
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
                WorkspaceOptions {
                    options: ClientOptions {
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
        "#);
    }

    #[cfg(not(windows))]
    #[test]
    fn test_vs_code_workspace_settings_resolve() {
        let options = deserialize_fixture(VS_CODE_INIT_OPTIONS_FIXTURE);
        let AllOptions {
            global,
            workspace: workspace_options,
        } = AllOptions::from_init_options(options);
        let path =
            Url::from_str("file:///Users/test/projects/pandas").expect("path should be valid");
        let all_workspace_options = workspace_options.expect("workspace options should exist");

        let workspace_options = all_workspace_options
            .get(&path)
            .expect("workspace options should exist")
            .clone();
        let workspace_settings = workspace_options
            .combine(global.client().clone())
            .into_settings()
            .unwrap();

        assert_eq!(
            workspace_settings,
            ClientSettings {
                fix_all: true,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: false,
                fix_violation_enable: false,
                show_syntax_errors: true,
                editor_settings: EditorSettings {
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
        let workspace_options = all_workspace_options
            .get(&path)
            .expect("workspace setting should exist")
            .clone();

        let workspace_settings = workspace_options
            .combine(global.client().clone())
            .into_settings()
            .unwrap();

        assert_eq!(
            workspace_settings,
            ClientSettings {
                fix_all: true,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: true,
                fix_violation_enable: false,
                show_syntax_errors: true,
                editor_settings: EditorSettings {
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
            settings: GlobalOptions {
                client: ClientOptions {
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
                },
                tracing: TracingOptions {
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
        let (main_loop_sender, main_loop_receiver) = crossbeam::channel::unbounded();
        let (client_sender, client_receiver) = crossbeam::channel::unbounded();

        let options = deserialize_fixture(GLOBAL_ONLY_INIT_OPTIONS_FIXTURE);

        let AllOptions { global, .. } = AllOptions::from_init_options(options);
        let client = Client::new(main_loop_sender, client_sender);
        let global = global.into_settings(client);
        assert_eq!(
            global.to_settings(),
            &ClientSettings {
                fix_all: false,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: false,
                fix_violation_enable: true,
                show_syntax_errors: true,
                editor_settings: EditorSettings {
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

        assert!(main_loop_receiver.is_empty());
        assert!(client_receiver.is_empty());
    }

    #[test]
    fn test_empty_init_options_deserialize() {
        let options: InitializationOptions = deserialize_fixture(EMPTY_INIT_OPTIONS_FIXTURE);

        assert_eq!(options, InitializationOptions::default());
    }

    fn assert_preview_client_options(options: &ClientOptions, preview: bool) {
        assert_eq!(options.lint.as_ref().unwrap().preview.unwrap(), preview);
        assert_eq!(options.format.as_ref().unwrap().preview.unwrap(), preview);
    }

    fn assert_preview_all_options(all_options: &AllOptions, preview: bool) {
        assert_preview_client_options(all_options.global.client(), preview);
        if let Some(workspace_options) = all_options.workspace.as_ref() {
            for options in workspace_options.values() {
                assert_preview_client_options(options, preview);
            }
        }
    }

    #[test]
    fn test_preview_flag() {
        let options = deserialize_fixture(EMPTY_MULTIPLE_WORKSPACE_INIT_OPTIONS_FIXTURE);
        let mut all_options = AllOptions::from_init_options(options);

        all_options.set_preview(false);
        assert_preview_all_options(&all_options, false);

        all_options.set_preview(true);
        assert_preview_all_options(&all_options, true);
    }

    #[test]
    fn inline_configuration() {
        let (main_loop_sender, main_loop_receiver) = crossbeam::channel::unbounded();
        let (client_sender, client_receiver) = crossbeam::channel::unbounded();
        let client = Client::new(main_loop_sender, client_sender);

        let options: InitializationOptions = deserialize_fixture(INLINE_CONFIGURATION_FIXTURE);

        let AllOptions {
            global,
            workspace: None,
        } = AllOptions::from_init_options(options)
        else {
            panic!("Expected global settings only");
        };

        let global = global.into_settings(client);

        assert_eq!(
            global.to_settings(),
            &ClientSettings {
                fix_all: true,
                organize_imports: true,
                lint_enable: true,
                disable_rule_comment_enable: true,
                fix_violation_enable: true,
                show_syntax_errors: true,
                editor_settings: EditorSettings {
                    configuration: Some(ResolvedConfiguration::Inline(Box::new(Options {
                        line_length: Some(LineLength::try_from(100).unwrap()),
                        lint: Some(LintOptions {
                            common: LintCommonOptions {
                                extend_select: Some(vec![RuleSelector::from_str("I001").unwrap()]),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        format: Some(RuffFormatOptions {
                            quote_style: Some(QuoteStyle::Single),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }))),
                    extend_select: Some(vec![RuleSelector::from_str("RUF001").unwrap()]),
                    ..Default::default()
                }
            }
        );

        assert!(main_loop_receiver.is_empty());
        assert!(client_receiver.is_empty());
    }
}
