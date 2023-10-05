#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use ruff_text_size::{Ranged, TextSize};

use crate::edit::Edit;

/// Indicates if a fix can be applied.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Applicability {
    /// The fix is safe and can always be applied.
    /// The fix is definitely what the user intended, or it maintains the exact meaning of the code.
    Always,

    /// The fix is unsafe and should only be applied with user opt-in.
    /// The fix may be what the user intended, but it is uncertain; the resulting code will have valid syntax.
    Sometimes,

    /// The fix is unsafe and should only be manually applied by the user.
    /// The fix is likely to be incorrect or the resulting code may have invalid syntax.
    Never,
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
    /// Create a new [`Fix`] that can [always](Applicability::Always) be applied from an [`Edit`] element.
    pub fn always_applies(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Always,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] that can [always](Applicability::Always) be applied from multiple [`Edit`] elements.
    pub fn always_applies_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(|edit| (edit.start(), edit.end()));
        Self {
            edits,
            applicability: Applicability::Always,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] that can [sometimes](Applicability::Sometimes) be applied from an [`Edit`] element.
    pub fn sometimes_applies(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Sometimes,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] that can [sometimes](Applicability::Sometimes) be applied from multiple [`Edit`] elements.
    pub fn sometimes_applies_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(|edit| (edit.start(), edit.end()));
        Self {
            edits,
            applicability: Applicability::Sometimes,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] that should [never](Applicability::Never) be applied from an [`Edit`] element .
    pub fn never_applies(edit: Edit) -> Self {
        Self {
            edits: vec![edit],
            applicability: Applicability::Never,
            isolation_level: IsolationLevel::default(),
        }
    }

    /// Create a new [`Fix`] that should [never](Applicability::Never) be applied from multiple [`Edit`] elements.
    pub fn never_applies_edits(edit: Edit, rest: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = std::iter::once(edit).chain(rest).collect();
        edits.sort_by_key(|edit| (edit.start(), edit.end()));
        Self {
            edits,
            applicability: Applicability::Never,
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
