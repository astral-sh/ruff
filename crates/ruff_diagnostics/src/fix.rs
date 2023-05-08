use ruff_text_size::TextSize;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::edit::Edit;

/// Indicates confidence in the correctness of a suggested fix.
#[derive(Default, Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum Applicability {
    /// The fix is definitely what the user intended, or maintains the exact meaning of the code.
    /// This fix should be automatically applied.
    Automatic,

    /// The fix may be what the user intended, but it is uncertain.
    /// The fix should result in valid code if it is applied.
    /// The fix can be applied with user opt-in.
    Suggested,

    /// The fix has a good chance of being incorrect or the code be incomplete.
    /// The fix may result in invalid code if it is applied.
    /// The fix should only be manually applied by the user.
    Manual,

    /// The applicability of the fix is unknown.
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

    /// Create a new [`Fix`] with automatic applicability from an [`Edit`] element.
    pub fn automatic(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Automatic,
        }
    }

    /// Create a new [`Fix`] with automatic applicability from multiple [`Edit`] elements.
    pub fn automatic_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest.into_iter()).collect(),
            applicability: Applicability::Automatic,
        }
    }

    /// Create a new [`Fix`] with suggsted applicability from an [`Edit`] element.
    pub fn suggested(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Suggested,
        }
    }

    /// Create a new [`Fix`] with suggsted applicability from multiple [`Edit`] elements.
    pub fn suggested_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest.into_iter()).collect(),
            applicability: Applicability::Suggested,
        }
    }


    /// Create a new [`Fix`] with manual applicability from an [`Edit`] element.
    pub fn manual(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Manual,
        }
    }

    /// Create a new [`Fix`] with manual applicability from multiple [`Edit`] elements.
    pub fn manual_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest.into_iter()).collect(),
            applicability: Applicability::Manual,
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
