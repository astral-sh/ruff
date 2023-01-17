//! Settings for the `pydocstyle` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::RuleCodePrefix;

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
    pub fn codes(self) -> &'static [RuleCodePrefix] {
        match self {
            Convention::Google => &[
                // All errors except D203, D204, D213, D215, D400, D401, D404, D406, D407, D408,
                // D409 and D413.
                RuleCodePrefix::D203,
                RuleCodePrefix::D204,
                RuleCodePrefix::D213,
                RuleCodePrefix::D215,
                RuleCodePrefix::D400,
                RuleCodePrefix::D404,
                RuleCodePrefix::D406,
                RuleCodePrefix::D407,
                RuleCodePrefix::D408,
                RuleCodePrefix::D409,
                RuleCodePrefix::D413,
            ],
            Convention::Numpy => &[
                // All errors except D107, D203, D212, D213, D402, D413, D415, D416, and D417.
                RuleCodePrefix::D107,
                RuleCodePrefix::D203,
                RuleCodePrefix::D212,
                RuleCodePrefix::D213,
                RuleCodePrefix::D402,
                RuleCodePrefix::D413,
                RuleCodePrefix::D415,
                RuleCodePrefix::D416,
                RuleCodePrefix::D417,
            ],
            Convention::Pep257 => &[
                // All errors except D203, D212, D213, D214, D215, D404, D405, D406, D407, D408,
                // D409, D410, D411, D413, D415, D416 and D417.
                RuleCodePrefix::D203,
                RuleCodePrefix::D212,
                RuleCodePrefix::D213,
                RuleCodePrefix::D214,
                RuleCodePrefix::D215,
                RuleCodePrefix::D404,
                RuleCodePrefix::D405,
                RuleCodePrefix::D406,
                RuleCodePrefix::D407,
                RuleCodePrefix::D408,
                RuleCodePrefix::D409,
                RuleCodePrefix::D410,
                RuleCodePrefix::D411,
                RuleCodePrefix::D413,
                RuleCodePrefix::D415,
                RuleCodePrefix::D416,
                RuleCodePrefix::D417,
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
