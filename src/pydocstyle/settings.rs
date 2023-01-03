//! Settings for the `pydocstyle` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::CheckCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Convention {
    /// Use Google-style docstrings.
    Google,
    /// Use NumPy-style docstrings.
    Numpy,
    /// Use PEP257-style docstrings.
    Pep257,
}

impl Convention {
    pub fn codes(&self) -> Vec<CheckCode> {
        match self {
            Convention::Google => vec![
                // All errors except D203, D204, D213, D215, D400, D401, D404, D406, D407, D408,
                // D409 and D413.
                CheckCode::D100,
                CheckCode::D101,
                CheckCode::D102,
                CheckCode::D103,
                CheckCode::D104,
                CheckCode::D105,
                CheckCode::D106,
                CheckCode::D107,
                CheckCode::D200,
                CheckCode::D201,
                CheckCode::D202,
                // CheckCode::D203,
                // CheckCode::D204,
                CheckCode::D205,
                CheckCode::D206,
                CheckCode::D207,
                CheckCode::D208,
                CheckCode::D209,
                CheckCode::D210,
                CheckCode::D211,
                CheckCode::D212,
                // CheckCode::D213,
                CheckCode::D214,
                // CheckCode::D215,
                CheckCode::D300,
                CheckCode::D301,
                // CheckCode::D400,
                CheckCode::D402,
                CheckCode::D403,
                // CheckCode::D404,
                CheckCode::D405,
                // CheckCode::D406,
                // CheckCode::D407,
                // CheckCode::D408,
                // CheckCode::D409,
                CheckCode::D410,
                CheckCode::D411,
                CheckCode::D412,
                // CheckCode::D413,
                CheckCode::D414,
                CheckCode::D415,
                CheckCode::D416,
                CheckCode::D417,
                CheckCode::D418,
                CheckCode::D419,
            ],
            Convention::Numpy => vec![
                // All errors except D107, D203, D212, D213, D402, D413, D415, D416, and D417.
                CheckCode::D100,
                CheckCode::D101,
                CheckCode::D102,
                CheckCode::D103,
                CheckCode::D104,
                CheckCode::D105,
                CheckCode::D106,
                // CheckCode::D107,
                CheckCode::D200,
                CheckCode::D201,
                CheckCode::D202,
                // CheckCode::D203,
                CheckCode::D204,
                CheckCode::D205,
                CheckCode::D206,
                CheckCode::D207,
                CheckCode::D208,
                CheckCode::D209,
                CheckCode::D210,
                CheckCode::D211,
                // CheckCode::D212,
                // CheckCode::D213,
                CheckCode::D214,
                CheckCode::D215,
                CheckCode::D300,
                CheckCode::D301,
                CheckCode::D400,
                // CheckCode::D402,
                CheckCode::D403,
                CheckCode::D404,
                CheckCode::D405,
                CheckCode::D406,
                CheckCode::D407,
                CheckCode::D408,
                CheckCode::D409,
                CheckCode::D410,
                CheckCode::D411,
                CheckCode::D412,
                // CheckCode::D413,
                CheckCode::D414,
                // CheckCode::D415,
                // CheckCode::D416,
                // CheckCode::D417,
                CheckCode::D418,
                CheckCode::D419,
            ],
            Convention::Pep257 => vec![
                // All errors except D203, D212, D213, D214, D215, D404, D405, D406, D407, D408,
                // D409, D410, D411, D413, D415, D416 and D417.
                CheckCode::D100,
                CheckCode::D101,
                CheckCode::D102,
                CheckCode::D103,
                CheckCode::D104,
                CheckCode::D105,
                CheckCode::D106,
                CheckCode::D107,
                CheckCode::D200,
                CheckCode::D201,
                CheckCode::D202,
                // CheckCode::D203,
                CheckCode::D204,
                CheckCode::D205,
                CheckCode::D206,
                CheckCode::D207,
                CheckCode::D208,
                CheckCode::D209,
                CheckCode::D210,
                CheckCode::D211,
                // CheckCode::D212,
                // CheckCode::D213,
                // CheckCode::D214,
                // CheckCode::D215,
                CheckCode::D300,
                CheckCode::D301,
                CheckCode::D400,
                CheckCode::D402,
                CheckCode::D403,
                // CheckCode::D404,
                // CheckCode::D405,
                // CheckCode::D406,
                // CheckCode::D407,
                // CheckCode::D408,
                // CheckCode::D409,
                // CheckCode::D410,
                // CheckCode::D411,
                CheckCode::D412,
                // CheckCode::D413,
                CheckCode::D414,
                // CheckCode::D415,
                // CheckCode::D416,
                // CheckCode::D417,
                CheckCode::D418,
                CheckCode::D419,
            ],
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", rename = "Pydocstyle")]
pub struct Options {
    #[option(
        default = r#"None"#,
        value_type = "Convention",
        example = r#"
            # Use Google-style docstrings.
            convention = "google"
        "#
    )]
    /// Whether to use Google-style or NumPy-style conventions or the PEP257
    /// defaults when analyzing docstring sections.
    pub convention: Option<Convention>,
}

#[derive(Debug, Default, Hash)]
pub struct Settings {
    pub convention: Option<Convention>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            convention: options.convention,
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            convention: settings.convention,
        }
    }
}
