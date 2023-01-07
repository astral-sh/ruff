//! Settings for the `pydocstyle` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::DiagnosticCodePrefix;

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
    pub fn codes(&self) -> &'static [DiagnosticCodePrefix] {
        match self {
            Convention::Google => &[
                // All errors except D203, D204, D213, D215, D400, D401, D404, D406, D407, D408,
                // D409 and D413.
                DiagnosticCodePrefix::D203,
                DiagnosticCodePrefix::D204,
                DiagnosticCodePrefix::D213,
                DiagnosticCodePrefix::D215,
                DiagnosticCodePrefix::D400,
                DiagnosticCodePrefix::D404,
                DiagnosticCodePrefix::D406,
                DiagnosticCodePrefix::D407,
                DiagnosticCodePrefix::D408,
                DiagnosticCodePrefix::D409,
                DiagnosticCodePrefix::D413,
            ],
            Convention::Numpy => &[
                // All errors except D107, D203, D212, D213, D402, D413, D415, D416, and D417.
                DiagnosticCodePrefix::D107,
                DiagnosticCodePrefix::D203,
                DiagnosticCodePrefix::D212,
                DiagnosticCodePrefix::D213,
                DiagnosticCodePrefix::D402,
                DiagnosticCodePrefix::D413,
                DiagnosticCodePrefix::D415,
                DiagnosticCodePrefix::D416,
                DiagnosticCodePrefix::D417,
            ],
            Convention::Pep257 => &[
                // All errors except D203, D212, D213, D214, D215, D404, D405, D406, D407, D408,
                // D409, D410, D411, D413, D415, D416 and D417.
                DiagnosticCodePrefix::D203,
                DiagnosticCodePrefix::D212,
                DiagnosticCodePrefix::D213,
                DiagnosticCodePrefix::D214,
                DiagnosticCodePrefix::D215,
                DiagnosticCodePrefix::D404,
                DiagnosticCodePrefix::D405,
                DiagnosticCodePrefix::D406,
                DiagnosticCodePrefix::D407,
                DiagnosticCodePrefix::D408,
                DiagnosticCodePrefix::D409,
                DiagnosticCodePrefix::D410,
                DiagnosticCodePrefix::D411,
                DiagnosticCodePrefix::D413,
                DiagnosticCodePrefix::D415,
                DiagnosticCodePrefix::D416,
                DiagnosticCodePrefix::D417,
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
