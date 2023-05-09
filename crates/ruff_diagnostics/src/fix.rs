use ruff_text_size::TextSize;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::edit::Edit;

/// A collection of [`Edit`] elements to be applied to a source file.
#[derive(Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Fix {
    edits: Vec<Edit>,
}

impl Fix {
    /// Create a new [`Fix`] with an unspecified applicability from an [`Edit`] element.
    pub fn unspecified(edit: Edit) -> Self {
        Self { edits: vec![edit] }
    }

    /// Create a new [`Fix`] with unspecified applicability from multiple [`Edit`] elements.
    pub fn unspecified_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest.into_iter()).collect(),
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
