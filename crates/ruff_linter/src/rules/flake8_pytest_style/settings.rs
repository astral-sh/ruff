//! Settings for the `flake8-pytest-style` plugin.
use std::error::Error;
use std::fmt;
use std::fmt::Formatter;

use crate::display_settings;
use ruff_macros::CacheKey;

use crate::settings::types::IdentifierPattern;

use super::types;

pub fn default_broad_exceptions() -> Vec<IdentifierPattern> {
    [
        "BaseException",
        "Exception",
        "ValueError",
        "OSError",
        "IOError",
        "EnvironmentError",
        "socket.error",
    ]
    .map(|pattern| IdentifierPattern::new(pattern).expect("invalid default exception pattern"))
    .to_vec()
}

pub fn default_broad_warnings() -> Vec<IdentifierPattern> {
    ["Warning", "UserWarning", "DeprecationWarning"]
        .map(|pattern| IdentifierPattern::new(pattern).expect("invalid default warning pattern"))
        .to_vec()
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub fixture_parentheses: bool,
    pub parametrize_names_type: types::ParametrizeNameType,
    pub parametrize_values_type: types::ParametrizeValuesType,
    pub parametrize_values_row_type: types::ParametrizeValuesRowType,
    pub raises_require_match_for: Vec<IdentifierPattern>,
    pub raises_extend_require_match_for: Vec<IdentifierPattern>,
    pub mark_parentheses: bool,
    pub warns_require_match_for: Vec<IdentifierPattern>,
    pub warns_extend_require_match_for: Vec<IdentifierPattern>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fixture_parentheses: false,
            parametrize_names_type: types::ParametrizeNameType::default(),
            parametrize_values_type: types::ParametrizeValuesType::default(),
            parametrize_values_row_type: types::ParametrizeValuesRowType::default(),
            raises_require_match_for: default_broad_exceptions(),
            raises_extend_require_match_for: vec![],
            mark_parentheses: false,
            warns_require_match_for: default_broad_warnings(),
            warns_extend_require_match_for: vec![],
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_pytest_style",
            fields = [
                self.fixture_parentheses,
                self.parametrize_names_type,
                self.parametrize_values_type,
                self.parametrize_values_row_type,
                self.raises_require_match_for | array,
                self.raises_extend_require_match_for | array,
                self.mark_parentheses
            ]
        }
        Ok(())
    }
}

/// Error returned by the [`TryFrom`] implementation of [`Settings`].
#[derive(Debug)]
pub enum SettingsError {
    InvalidRaisesRequireMatchFor(glob::PatternError),
    InvalidRaisesExtendRequireMatchFor(glob::PatternError),
    InvalidWarnsRequireMatchFor(glob::PatternError),
    InvalidWarnsExtendRequireMatchFor(glob::PatternError),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::InvalidRaisesRequireMatchFor(err) => {
                write!(f, "invalid raises-require-match-for pattern: {err}")
            }
            SettingsError::InvalidRaisesExtendRequireMatchFor(err) => {
                write!(f, "invalid raises-extend-require-match-for pattern: {err}")
            }
            SettingsError::InvalidWarnsRequireMatchFor(err) => {
                write!(f, "invalid warns-require-match-for pattern: {err}")
            }
            SettingsError::InvalidWarnsExtendRequireMatchFor(err) => {
                write!(f, "invalid warns-extend-require-match-for pattern: {err}")
            }
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SettingsError::InvalidRaisesRequireMatchFor(err) => Some(err),
            SettingsError::InvalidRaisesExtendRequireMatchFor(err) => Some(err),
            SettingsError::InvalidWarnsRequireMatchFor(err) => Some(err),
            SettingsError::InvalidWarnsExtendRequireMatchFor(err) => Some(err),
        }
    }
}
