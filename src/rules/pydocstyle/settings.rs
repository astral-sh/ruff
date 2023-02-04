//! Settings for the `pydocstyle` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::Rule;

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
    pub const fn rules_to_be_ignored(self) -> &'static [Rule] {
        match self {
            Convention::Google => &[
                Rule::OneBlankLineBeforeClass,
                Rule::OneBlankLineAfterClass,
                Rule::MultiLineSummarySecondLine,
                Rule::SectionUnderlineNotOverIndented,
                Rule::EndsInPeriod,
                Rule::NoThisPrefix,
                Rule::NewLineAfterSectionName,
                Rule::DashedUnderlineAfterSection,
                Rule::SectionUnderlineAfterName,
                Rule::SectionUnderlineMatchesSectionLength,
                Rule::BlankLineAfterLastSection,
            ],
            Convention::Numpy => &[
                Rule::PublicInit,
                Rule::OneBlankLineBeforeClass,
                Rule::MultiLineSummaryFirstLine,
                Rule::MultiLineSummarySecondLine,
                Rule::NoSignature,
                Rule::BlankLineAfterLastSection,
                Rule::EndsInPunctuation,
                Rule::SectionNameEndsInColon,
                Rule::DocumentAllArguments,
            ],
            Convention::Pep257 => &[
                Rule::OneBlankLineBeforeClass,
                Rule::MultiLineSummaryFirstLine,
                Rule::MultiLineSummarySecondLine,
                Rule::SectionNotOverIndented,
                Rule::SectionUnderlineNotOverIndented,
                Rule::NoThisPrefix,
                Rule::CapitalizeSectionName,
                Rule::NewLineAfterSectionName,
                Rule::DashedUnderlineAfterSection,
                Rule::SectionUnderlineAfterName,
                Rule::SectionUnderlineMatchesSectionLength,
                Rule::BlankLineAfterSection,
                Rule::BlankLineBeforeSection,
                Rule::BlankLineAfterLastSection,
                Rule::EndsInPunctuation,
                Rule::SectionNameEndsInColon,
                Rule::DocumentAllArguments,
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
        value_type = r#""google" | "numpy" | "pep257""#,
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
