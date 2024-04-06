//! Settings for the `pydocstyle` plugin.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::display_settings;
use ruff_macros::CacheKey;

use crate::registry::Rule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

impl fmt::Display for Convention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Google => write!(f, "google"),
            Self::Numpy => write!(f, "numpy"),
            Self::Pep257 => write!(f, "pep257"),
        }
    }
}

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub convention: Option<Convention>,
    pub ignore_decorators: BTreeSet<String>,
    pub property_decorators: BTreeSet<String>,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pydocstyle",
            fields = [
                self.convention | optional,
                self.ignore_decorators | set,
                self.property_decorators | set
            ]
        }
        Ok(())
    }
}
