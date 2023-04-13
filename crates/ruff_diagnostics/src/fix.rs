use rustpython_parser::ast::Location;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::edit::Edit;

/// A collection of [`Edit`] elements to be applied to a source file.
#[derive(Default, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Fix {
    edits: Vec<Edit>,
}

impl Fix {
    /// Create a new [`Fix`] from a vector of [`Edit`] elements.
    pub fn new(edits: Vec<Edit>) -> Self {
        Self { edits }
    }

    /// Create an empty [`Fix`].
    pub const fn empty() -> Self {
        Self { edits: Vec::new() }
    }

    /// Return `true` if the [`Fix`] contains no [`Edit`] elements.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Return the [`Location`] of the first [`Edit`] in the [`Fix`].
    pub fn min_location(&self) -> Option<Location> {
        self.edits.iter().map(Edit::location).min()
    }

    /// Return a slice of the [`Edit`] elements in the [`Fix`].
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    pub fn into_edits(self) -> Vec<Edit> {
        self.edits
    }
}

impl FromIterator<Edit> for Fix {
    fn from_iter<T: IntoIterator<Item = Edit>>(iter: T) -> Self {
        Self {
            edits: Vec::from_iter(iter),
        }
    }
}

impl From<Edit> for Fix {
    fn from(edit: Edit) -> Self {
        Self { edits: vec![edit] }
    }
}
