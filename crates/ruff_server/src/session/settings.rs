use std::{path::PathBuf, sync::Arc};

use thiserror::Error;

use ruff_linter::RuleSelector;
use ruff_linter::line_width::LineLength;
use ruff_workspace::options::Options;

use crate::{
    ClientOptions,
    session::{
        Client,
        options::{ClientConfiguration, ConfigurationPreference},
    },
};

pub struct GlobalClientSettings {
    pub(super) options: ClientOptions,

    /// Lazily initialized client settings to avoid showing error warnings
    /// when a field of the global settings has any errors but the field is overridden
    /// in the workspace settings. This can avoid showing unnecessary errors
    /// when the workspace settings e.g. select some rules that aren't available in a specific workspace
    /// and said workspace overrides the selected rules.
    pub(super) settings: std::cell::OnceCell<Arc<ClientSettings>>,

    pub(super) client: Client,
}

impl GlobalClientSettings {
    pub(super) fn options(&self) -> &ClientOptions {
        &self.options
    }

    fn settings_impl(&self) -> &Arc<ClientSettings> {
        self.settings.get_or_init(|| {
            let settings = self.options.clone().into_settings();
            let settings = match settings {
                Ok(settings) => settings,
                Err(settings) => {
                    self.client.show_error_message(
                        "Ruff received invalid settings from the editor. Refer to the logs for more information."
                    );
                    settings
                }
            };
            Arc::new(settings)
        })
    }

    /// Lazily resolves the client options to the settings.
    pub(super) fn to_settings(&self) -> &ClientSettings {
        self.settings_impl()
    }

    /// Lazily resolves the client options to the settings.
    pub(super) fn to_settings_arc(&self) -> Arc<ClientSettings> {
        self.settings_impl().clone()
    }
}

/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[expect(clippy::struct_excessive_bools)]
pub(crate) struct ClientSettings {
    pub(super) fix_all: bool,
    pub(super) organize_imports: bool,
    pub(super) lint_enable: bool,
    pub(super) disable_rule_comment_enable: bool,
    pub(super) fix_violation_enable: bool,
    pub(super) show_syntax_errors: bool,
    pub(super) editor_settings: EditorSettings,
}

/// Contains the resolved values of 'editor settings' - Ruff configuration for the linter/formatter that was passed in via
/// LSP client settings. These fields are optional because we don't want to override file-based linter/formatting settings
/// if these were un-set.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Default, PartialEq, Eq))]
pub(crate) struct EditorSettings {
    pub(super) configuration: Option<ResolvedConfiguration>,
    pub(super) lint_preview: Option<bool>,
    pub(super) format_preview: Option<bool>,
    pub(super) select: Option<Vec<RuleSelector>>,
    pub(super) extend_select: Option<Vec<RuleSelector>>,
    pub(super) ignore: Option<Vec<RuleSelector>>,
    pub(super) exclude: Option<Vec<String>>,
    pub(super) line_length: Option<LineLength>,
    pub(super) configuration_preference: ConfigurationPreference,
}

/// The resolved configuration from the client settings.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) enum ResolvedConfiguration {
    FilePath(PathBuf),
    Inline(Box<Options>),
}

impl TryFrom<ClientConfiguration> for ResolvedConfiguration {
    type Error = ResolvedConfigurationError;

    fn try_from(value: ClientConfiguration) -> Result<Self, Self::Error> {
        match value {
            ClientConfiguration::String(path) => Ok(ResolvedConfiguration::FilePath(
                PathBuf::from(shellexpand::full(&path)?.as_ref()),
            )),
            ClientConfiguration::Object(map) => {
                let options = toml::Table::try_from(map)?.try_into::<Options>()?;
                if options.extend.is_some() {
                    Err(ResolvedConfigurationError::ExtendNotSupported)
                } else {
                    Ok(ResolvedConfiguration::Inline(Box::new(options)))
                }
            }
        }
    }
}

/// An error that can occur when trying to resolve the `configuration` value from the client
/// settings.
#[derive(Debug, Error)]
pub(crate) enum ResolvedConfigurationError {
    #[error(transparent)]
    EnvVarLookupError(#[from] shellexpand::LookupError<std::env::VarError>),
    #[error("error serializing configuration to TOML: {0}")]
    InvalidToml(#[from] toml::ser::Error),
    #[error(transparent)]
    InvalidRuffSchema(#[from] toml::de::Error),
    #[error("using `extend` is unsupported for inline configuration")]
    ExtendNotSupported,
}

impl ClientSettings {
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

    pub(crate) fn editor_settings(&self) -> &EditorSettings {
        &self.editor_settings
    }
}
