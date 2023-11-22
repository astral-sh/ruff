//! Settings for the `pep8-naming` plugin.

use std::error::Error;
use std::fmt;

use ruff_macros::CacheKey;

use crate::settings::types::IdentifierPattern;

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub ignore_names: Vec<IdentifierPattern>,
    pub classmethod_decorators: Vec<String>,
    pub staticmethod_decorators: Vec<String>,
}

pub fn default_ignore_names() -> Vec<String> {
    vec![
        "setUp".to_string(),
        "tearDown".to_string(),
        "setUpClass".to_string(),
        "tearDownClass".to_string(),
        "setUpModule".to_string(),
        "tearDownModule".to_string(),
        "asyncSetUp".to_string(),
        "asyncTearDown".to_string(),
        "setUpTestData".to_string(),
        "failureException".to_string(),
        "longMessage".to_string(),
        "maxDiff".to_string(),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: default_ignore_names()
                .into_iter()
                .map(|name| IdentifierPattern::new(&name).unwrap())
                .collect(),
            classmethod_decorators: Vec::new(),
            staticmethod_decorators: Vec::new(),
        }
    }
}

/// Error returned by the [`TryFrom`] implementation of [`Settings`].
#[derive(Debug)]
pub enum SettingsError {
    InvalidIgnoreName(glob::PatternError),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::InvalidIgnoreName(err) => {
                write!(f, "Invalid pattern in ignore-names: {err}")
            }
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SettingsError::InvalidIgnoreName(err) => Some(err),
        }
    }
}
