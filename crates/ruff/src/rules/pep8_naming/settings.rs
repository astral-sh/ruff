//! Settings for the `pep8-naming` plugin.

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

use crate::settings::types::IdentifierPattern;

const IGNORE_NAMES: [&str; 12] = [
    "setUp",
    "tearDown",
    "setUpClass",
    "tearDownClass",
    "setUpModule",
    "tearDownModule",
    "asyncSetUp",
    "asyncTearDown",
    "setUpTestData",
    "failureException",
    "longMessage",
    "maxDiff",
];

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Pep8NamingOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = r#"["setUp", "tearDown", "setUpClass", "tearDownClass", "setUpModule", "tearDownModule", "asyncSetUp", "asyncTearDown", "setUpTestData", "failureException", "longMessage", "maxDiff"]"#,
        value_type = "list[str]",
        example = r#"
            ignore-names = ["callMethod"]
        "#
    )]
    /// A list of names (or patterns) to ignore when considering `pep8-naming` violations.
    pub ignore_names: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow Pydantic's `@validator` decorator to trigger class method treatment.
            classmethod-decorators = ["pydantic.validator"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a class method (in addition to the builtin
    /// `@classmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list takes a `cls` argument as its first argument.
    pub classmethod_decorators: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow a shorthand alias, `@stcmthd`, to trigger static method treatment.
            staticmethod-decorators = ["stcmthd"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a static method (in addition to the builtin
    /// `@staticmethod`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list has no `self` or `cls` argument.
    pub staticmethod_decorators: Option<Vec<String>>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub ignore_names: Vec<IdentifierPattern>,
    pub classmethod_decorators: Vec<String>,
    pub staticmethod_decorators: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IGNORE_NAMES
                .iter()
                .map(|name| IdentifierPattern::new(name).unwrap())
                .collect(),
            classmethod_decorators: Vec::new(),
            staticmethod_decorators: Vec::new(),
        }
    }
}

impl TryFrom<Options> for Settings {
    type Error = SettingsError;

    fn try_from(options: Options) -> Result<Self, Self::Error> {
        Ok(Self {
            ignore_names: match options.ignore_names {
                Some(names) => names
                    .into_iter()
                    .map(|name| {
                        IdentifierPattern::new(&name).map_err(SettingsError::InvalidIgnoreName)
                    })
                    .collect::<Result<Vec<_>, Self::Error>>()?,
                None => IGNORE_NAMES
                    .into_iter()
                    .map(|name| IdentifierPattern::new(name).unwrap())
                    .collect(),
            },
            classmethod_decorators: options.classmethod_decorators.unwrap_or_default(),
            staticmethod_decorators: options.staticmethod_decorators.unwrap_or_default(),
        })
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

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ignore_names: Some(
                settings
                    .ignore_names
                    .into_iter()
                    .map(|pattern| pattern.as_str().to_owned())
                    .collect(),
            ),
            classmethod_decorators: Some(settings.classmethod_decorators),
            staticmethod_decorators: Some(settings.staticmethod_decorators),
        }
    }
}
