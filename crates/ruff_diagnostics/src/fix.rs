#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use ruff_text_size::{Ranged, TextSize};

use crate::edit::Edit;

/// Indicates if a fix can be applied.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Applicability {
    /// The fix can be applied programmatically.
    /// The fix is likely to be correct and the resulting code will have valid syntax.
    Automatic(Safety),

    /// The fix should only be manually applied by the user.
    /// The fix is likely to be incorrect or the resulting code may have invalid syntax.
    Manual,
}

/// Indicates the safety of applying a fix.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[serde(rename_all = "lowercase")]
pub enum Safety {
    /// The fix is definitely what the user intended, or it maintains the exact meaning of the code.
    /// This fix can be automatically applied.
    Safe,
    /// The fix may be what the user intended, but it is uncertain.
    /// The fix can be applied with user opt-in.
    Unsafe,
}

/// Indicates the level of isolation required to apply a fix.
#[derive(Default, Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum IsolationLevel {
    /// The fix should be applied as long as no other fixes in the same group have been applied.
    Group(u32),
    /// The fix should be applied as long as it does not overlap with any other fixes.
    #[default]
    NonOverlapping,
}

/// A collection of [`Edit`] elements to be applied to a source file.
#[derive(Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Fix {
    /// The [`Edit`] elements to be applied, sorted by [`Edit::start`] in ascending order.
    edits: Vec<Edit>,
    /// The [`Applicability`] of the fix.
    applicability: Applicability,
    /// The [`IsolationLevel`] of the fix.
    isolation_level: IsolationLevel,
}

impl Fix {
    /// Create a new [`Fix`] with [safe applicability](Applicability::Automatic(Safety::Safe)) from an [`Edit`] element.
    pub fn automatic_safe(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Automatic(Safety::Safe),
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [safe applicability](Applicability::Automatic(Safety::Safe)) from multiple [`Edit`] elements.
    pub fn automatic_safe_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(|edit| (edit.start(), edit.end()));
        Self {
            edits,
            applicability: Applicability::Automatic(Safety::Safe),
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [unsafe applicability](Applicable::Automatic(Safety::Unsafe)) from an [`Edit`] element.
    pub fn automatic_unsafe(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Automatic(Safety::Unsafe),
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [unsafe applicability](Applicability::Automatic(Safety::Unsafe)) from multiple [`Edit`] elements.
    pub fn automatic_unsafe_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(|edit| (edit.start(), edit.end()));
        Self {
            edits,
            applicability: Applicability::Automatic(Safety::Unsafe),
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [manual applicability](Applicability::Manual) from an [`Edit`] element.
    pub fn manual(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Manual,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [manual applicability](Applicability::Manual) from multiple [`Edit`] elements.
    pub fn manual_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(|edit| (edit.start(), edit.end()));
        Self {
            edits,
            applicability: Applicability::Manual,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Return the [`TextSize`] of the first [`Edit`] in the [`Fix`].
    pub fn min_start(&self) -> Option<TextSize> {
        self.edits.first().map(Edit::start)
    }

    /// Return a slice of the [`Edit`] elements in the [`Fix`], sorted by [`Edit::start`] in ascending order.
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// Return the [`Applicability`] of the [`Fix`].
    pub fn applicability(&self) -> Applicability {
        self.applicability
    }

    /// Return the [`IsolationLevel`] of the [`Fix`].
    pub fn isolation(&self) -> IsolationLevel {
        self.isolation_level
    }

    /// Create a new [`Fix`] with the given [`IsolationLevel`].
    #[must_use]
    pub fn isolate(mut self, isolation: IsolationLevel) -> Self {
        self.isolation_level = isolation;
        self
    }
}
