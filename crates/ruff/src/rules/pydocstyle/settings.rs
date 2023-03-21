//! Settings for the `pydocstyle` plugin.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

use crate::registry::Rule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CacheKey)]
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
                Rule::NonImperativeMood,
                Rule::DocstringStartsWithThis,
                Rule::NewLineAfterSectionName,
                Rule::DashedUnderlineAfterSection,
                Rule::SectionUnderlineAfterName,
                Rule::SectionUnderlineMatchesSectionLength,
                Rule::BlankLineAfterLastSection,
            ],
            Convention::Numpy => &[
                Rule::UndocumentedPublicInit,
                Rule::OneBlankLineBeforeClass,
                Rule::MultiLineSummaryFirstLine,
                Rule::MultiLineSummarySecondLine,
                Rule::NoSignature,
                Rule::BlankLineAfterLastSection,
                Rule::EndsInPunctuation,
                Rule::SectionNameEndsInColon,
                Rule::UndocumentedParam,
            ],
            Convention::Pep257 => &[
                Rule::OneBlankLineBeforeClass,
                Rule::MultiLineSummaryFirstLine,
                Rule::MultiLineSummarySecondLine,
                Rule::SectionNotOverIndented,
                Rule::SectionUnderlineNotOverIndented,
                Rule::DocstringStartsWithThis,
                Rule::CapitalizeSectionName,
                Rule::NewLineAfterSectionName,
                Rule::DashedUnderlineAfterSection,
                Rule::SectionUnderlineAfterName,
                Rule::SectionUnderlineMatchesSectionLength,
                Rule::NoBlankLineAfterSection,
                Rule::NoBlankLineBeforeSection,
                Rule::BlankLineAfterLastSection,
                Rule::EndsInPunctuation,
                Rule::SectionNameEndsInColon,
                Rule::UndocumentedParam,
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
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            ignore-decorators = ["typing.overload"]
        "#
    )]
    /// Ignore docstrings for functions or methods decorated with the
    /// specified fully-qualified decorators.
    pub ignore_decorators: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            property-decorators = ["gi.repository.GObject.Property"]
        "#
    )]
    /// A list of decorators that, when applied to a method, indicate that the
    /// method should be treated as a property (in addition to the builtin
    /// `@property` and standard-library `@functools.cached_property`).
    ///
    /// For example, Ruff will expect that any method decorated by a decorator
    /// in this list can use a non-imperative summary line.
    pub property_decorators: Option<Vec<String>>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub convention: Option<Convention>,
    pub ignore_decorators: BTreeSet<String>,
    pub property_decorators: BTreeSet<String>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            convention: options.convention,
            ignore_decorators: BTreeSet::from_iter(options.ignore_decorators.unwrap_or_default()),
            property_decorators: BTreeSet::from_iter(
                options.property_decorators.unwrap_or_default(),
            ),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            convention: settings.convention,
            ignore_decorators: Some(settings.ignore_decorators.into_iter().collect()),
            property_decorators: Some(settings.property_decorators.into_iter().collect()),
        }
    }
}
