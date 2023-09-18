#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use ruff_text_size::{Ranged, TextSize};

use crate::edit::Edit;

/// Indicates confidence in the correctness of a suggested fix.
#[derive(Default, Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
    /// Create a new [`Fix`] with an unspecified applicability from an [`Edit`] element.
    #[deprecated(
        note = "Use `Fix::automatic`, `Fix::suggested`, or `Fix::manual` instead to specify an applicability."
    )]
    pub fn unspecified(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Unspecified,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with an unspecified applicability from multiple [`Edit`] elements.
    #[deprecated(
        note = "Use `Fix::automatic_edits`, `Fix::suggested_edits`, or `Fix::manual_edits` instead to specify an applicability."
    )]
    pub fn unspecified_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: std::iter::once(edit).chain(rest).collect(),
            applicability: Applicability::Unspecified,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [automatic applicability](Applicability::Automatic) from an [`Edit`] element.
    pub fn automatic(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Automatic,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [automatic applicability](Applicability::Automatic) from multiple [`Edit`] elements.
    pub fn automatic_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(Ranged::start);
        Self {
            edits,
            applicability: Applicability::Automatic,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [suggested applicability](Applicability::Suggested) from an [`Edit`] element.
    pub fn suggested(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Suggested,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] with [suggested applicability](Applicability::Suggested) from multiple [`Edit`] elements.
    pub fn suggested_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(Ranged::start);
        Self {
            edits,
            applicability: Applicability::Suggested,
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
        edits.sort_by_key(Ranged::start);
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
