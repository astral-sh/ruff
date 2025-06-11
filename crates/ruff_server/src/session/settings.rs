use std::{path::PathBuf, sync::Arc};

use thiserror::Error;

use ruff_linter::RuleSelector;
use ruff_linter::line_width::LineLength;
use ruff_workspace::options::Options;

use crate::{
    ClientOptions,
    session::options::{ClientConfiguration, ConfigurationPreference},
};

pub struct GlobalSettings {
    pub(super) options: ClientOptions,
    pub(super) settings: Arc<ClientSettings>,
}

impl GlobalSettings {
    pub(super) fn options(&self) -> &ClientOptions {
        &self.options
    }

    pub(super) fn settings(&self) -> &ClientSettings {
        &self.settings
    }

    pub(super) fn settings_arc(&self) -> Arc<ClientSettings> {
        self.settings.clone()
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
