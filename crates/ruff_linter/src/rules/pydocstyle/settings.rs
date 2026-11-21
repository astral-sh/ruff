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
                Rule::IncorrectBlankLineBeforeClassDocstring,
                Rule::IncorrectBlankLineAfterClassDocstring,
                Rule::MultilineDocstringSummaryOnFirstLine,
                Rule::OverIndentedDocstringSectionUnderline,
                Rule::DocstringSummaryWithoutPeriod,
                Rule::NonImperativeDocstringSummary,
                Rule::DocstringStartsWithThis,
                Rule::MissingNewlineAfterDocstringSectionName,
                Rule::MissingDocstringSectionUnderline,
                Rule::BlankLineBeforeDocstringSectionUnderline,
                Rule::IncorrectDocstringSectionUnderlineLength,
                Rule::MissingBlankLineAfterLastDocstringSection,
            ],
            Convention::Numpy => &[
                Rule::UndocumentedPublicInit,
                Rule::IncorrectBlankLineBeforeClassDocstring,
                Rule::MultilineDocstringSummaryOnSeparateLine,
                Rule::MultilineDocstringSummaryOnFirstLine,
                Rule::SignatureInDocstring,
                Rule::MissingBlankLineAfterLastDocstringSection,
                Rule::DocstringSummaryWithoutTerminalPunctuation,
                Rule::DocstringSectionNameWithoutColon,
                Rule::UndocumentedParameter,
            ],
            Convention::Pep257 => &[
                Rule::IncorrectBlankLineBeforeClassDocstring,
                Rule::MultilineDocstringSummaryOnSeparateLine,
                Rule::MultilineDocstringSummaryOnFirstLine,
                Rule::OverIndentedDocstringSection,
                Rule::OverIndentedDocstringSectionUnderline,
                Rule::DocstringStartsWithThis,
                Rule::IncorrectlyCapitalizedDocstringSectionName,
                Rule::MissingNewlineAfterDocstringSectionName,
                Rule::MissingDocstringSectionUnderline,
                Rule::BlankLineBeforeDocstringSectionUnderline,
                Rule::IncorrectDocstringSectionUnderlineLength,
                Rule::MissingBlankLineAfterDocstringSection,
                Rule::MissingBlankLineBeforeDocstringSection,
                Rule::MissingBlankLineAfterLastDocstringSection,
                Rule::DocstringSummaryWithoutTerminalPunctuation,
                Rule::DocstringSectionNameWithoutColon,
                Rule::UndocumentedParameter,
                Rule::IncorrectDocstringSectionOrder,
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
    pub(crate) fn convention(&self) -> Option<Convention> {
        self.convention
    }

    pub(crate) fn ignore_decorators(&self) -> DecoratorIterator<'_> {
        DecoratorIterator::new(&self.ignore_decorators)
    }

    pub(crate) fn property_decorators(&self) -> DecoratorIterator<'_> {
        DecoratorIterator::new(&self.property_decorators)
    }

    pub(crate) fn ignore_var_parameters(&self) -> bool {
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
