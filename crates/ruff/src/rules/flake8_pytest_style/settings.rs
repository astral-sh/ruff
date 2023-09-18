//! Settings for the `flake8-pytest-style` plugin.
use std::error::Error;
use std::fmt;

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

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub fixture_parentheses: bool,
    pub parametrize_names_type: types::ParametrizeNameType,
    pub parametrize_values_type: types::ParametrizeValuesType,
    pub parametrize_values_row_type: types::ParametrizeValuesRowType,
    pub raises_require_match_for: Vec<IdentifierPattern>,
    pub raises_extend_require_match_for: Vec<IdentifierPattern>,
    pub mark_parentheses: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fixture_parentheses: true,
            parametrize_names_type: types::ParametrizeNameType::default(),
            parametrize_values_type: types::ParametrizeValuesType::default(),
            parametrize_values_row_type: types::ParametrizeValuesRowType::default(),
            raises_require_match_for: default_broad_exceptions(),
            raises_extend_require_match_for: vec![],
            mark_parentheses: true,
        }
    }
}

/// Error returned by the [`TryFrom`] implementation of [`Settings`].
#[derive(Debug)]
pub enum SettingsError {
    InvalidRaisesRequireMatchFor(glob::PatternError),
    InvalidRaisesExtendRequireMatchFor(glob::PatternError),
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
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SettingsError::InvalidRaisesRequireMatchFor(err) => Some(err),
            SettingsError::InvalidRaisesExtendRequireMatchFor(err) => Some(err),
        }
    }
}
