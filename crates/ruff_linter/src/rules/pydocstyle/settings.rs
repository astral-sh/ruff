//! Settings for the `pydocstyle` plugin.

use std::collections::BTreeSet;
use std::fmt;
use std::iter::FusedIterator;

use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;
use ruff_python_ast::name::QualifiedName;

use crate::display_settings;
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
                Rule::IncorrectBlankLineBeforeClass,
                Rule::IncorrectBlankLineAfterClass,
                Rule::MultiLineSummarySecondLine,
                Rule::OverindentedSectionUnderline,
                Rule::MissingTrailingPeriod,
                Rule::NonImperativeMood,
                Rule::DocstringStartsWithThis,
                Rule::MissingNewLineAfterSectionName,
                Rule::MissingDashedUnderlineAfterSection,
                Rule::MissingSectionUnderlineAfterName,
                Rule::MismatchedSectionUnderlineLength,
                Rule::MissingBlankLineAfterLastSection,
            ],
            Convention::Numpy => &[
                Rule::UndocumentedPublicInit,
                Rule::IncorrectBlankLineBeforeClass,
                Rule::MultiLineSummaryFirstLine,
                Rule::MultiLineSummarySecondLine,
                Rule::SignatureInDocstring,
                Rule::MissingBlankLineAfterLastSection,
                Rule::MissingTerminalPunctuation,
                Rule::MissingSectionNameColon,
                Rule::UndocumentedParam,
            ],
            Convention::Pep257 => &[
                Rule::IncorrectBlankLineBeforeClass,
                Rule::MultiLineSummaryFirstLine,
                Rule::MultiLineSummarySecondLine,
                Rule::OverindentedSection,
                Rule::OverindentedSectionUnderline,
                Rule::DocstringStartsWithThis,
                Rule::NonCapitalizedSectionName,
                Rule::MissingNewLineAfterSectionName,
                Rule::MissingDashedUnderlineAfterSection,
                Rule::MissingSectionUnderlineAfterName,
                Rule::MismatchedSectionUnderlineLength,
                Rule::NoBlankLineAfterSection,
                Rule::NoBlankLineBeforeSection,
                Rule::MissingBlankLineAfterLastSection,
                Rule::MissingTerminalPunctuation,
                Rule::MissingSectionNameColon,
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
    pub ignore_var_parameters: bool,
}

impl Settings {
    pub fn convention(&self) -> Option<Convention> {
        self.convention
    }

    pub fn ignore_decorators(&self) -> DecoratorIterator {
        DecoratorIterator::new(&self.ignore_decorators)
    }

    pub fn property_decorators(&self) -> DecoratorIterator {
        DecoratorIterator::new(&self.property_decorators)
    }

    pub fn ignore_var_parameters(&self) -> bool {
        self.ignore_var_parameters
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pydocstyle",
            fields = [
                self.convention | optional,
                self.ignore_decorators | set,
                self.property_decorators | set,
                self.ignore_var_parameters
            ]
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DecoratorIterator<'a> {
    decorators: std::collections::btree_set::Iter<'a, String>,
}

impl<'a> DecoratorIterator<'a> {
    fn new(decorators: &'a BTreeSet<String>) -> Self {
        Self {
            decorators: decorators.iter(),
        }
    }
}

impl<'a> Iterator for DecoratorIterator<'a> {
    type Item = QualifiedName<'a>;

    fn next(&mut self) -> Option<QualifiedName<'a>> {
        self.decorators
            .next()
            .map(|deco| QualifiedName::from_dotted_name(deco))
    }
}

impl FusedIterator for DecoratorIterator<'_> {}

impl ExactSizeIterator for DecoratorIterator<'_> {
    fn len(&self) -> usize {
        self.decorators.len()
    }
}
