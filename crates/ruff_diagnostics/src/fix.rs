use ruff_text_size::TextSize;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::edit::Edit;

/// Indicates confidence in the correctness of a suggested fix.
#[derive(Default, Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum Applicability {
    /// The suggestion is definitely what the user intended, or maintains the exact meaning of the code.
    /// This suggestion should be automatically applied.
    Safe,

    /// The suggestion may be what the user intended, but it is uncertain.
    /// The suggestion should result in valid code if it is applied.
    MaybeIncorrect,

    /// The applicability of the suggestion is unknown.
    #[default]
    Unspecified,
}

/// A collection of [`Edit`] elements to be applied to a source file.
#[derive(Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Fix {
    edits: Vec<Edit>,
    applicability: Applicability,
}

impl Fix {
    /// Create a new [`Fix`] with an unspecified applicability from an [`Edit`] element.
    pub fn unspecified(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Unspecified,
        }
    }

    /// Create a new [`Fix`] with an unspecified applicability from multiple [`Edit`] elements.
    pub fn unspecified_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest.into_iter()).collect(),
            applicability: Applicability::Unspecified,
        }
    }

    /// Create a new [`Fix`] with safe applicability from an [`Edit`] element.
    pub fn safe(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::safe,
        }
    }

    /// Create a new [`Fix`] with safe applicability from multiple [`Edit`] elements.
    pub fn safe_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest.into_iter()).collect(),
            applicability: Applicability::safe,
        }
    }

    /// Create a new [`Fix`] with maybe incorrect applicability from an [`Edit`] element.
    pub fn maybe_incorrect(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::MaybeIncorrect,
        }
    }

    /// Create a new [`Fix`] with maybe incorrect applicability from multiple [`Edit`] elements.
    pub fn maybe_incorrect_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest.into_iter()).collect(),
            applicability: Applicability::MaybeIncorrect,
        }
    }

    /// Return the [`TextSize`] of the first [`Edit`] in the [`Fix`].
    pub fn min_start(&self) -> Option<TextSize> {
        self.edits.iter().map(Edit::start).min()
    }

    /// Return a slice of the [`Edit`] elements in the [`Fix`].
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    pub fn into_edits(self) -> Vec<Edit> {
        self.edits
    }
}
