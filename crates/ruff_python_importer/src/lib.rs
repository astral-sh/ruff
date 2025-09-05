/*!
Low level helpers for manipulating Python import statements.
*/

use ruff_text_size::{TextRange, TextSize};

pub use self::insertion::Insertion;

mod insertion;

/// A text edit to be applied to a source file.
///
/// Inserts, deletes, or replaces content at a given location.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Edit {
    /// The start location of the edit.
    range: TextRange,
    /// The replacement content to insert between the start and end locations.
    content: Option<Box<str>>,
}

impl Edit {
    /// Creates an edit that deletes the content in the `start` to `end` range.
    #[inline]
    pub fn deletion(range: TextRange) -> Self {
        Self {
            content: None,
            range,
        }
    }

    /// Creates an edit that replaces the content in `range` with `content`.
    ///
    /// When `content` is empty, this is equivalent to calling
    /// `Edit::deletion` with the given range.
    pub fn replacement(range: TextRange, content: impl Into<Box<str>>) -> Self {
        let content = content.into();
        if content.is_empty() {
            Self::deletion(range)
        } else {
            Self {
                content: Some(Box::from(content)),
                range,
            }
        }
    }

    /// Creates an edit that inserts `content` at the [`TextSize`] `at`.
    ///
    /// When `content` is empty, this is equivalent to calling
    /// `Edit::deletion` with the given range.
    pub fn insertion(at: TextSize, content: String) -> Self {
        if content.is_empty() {
            Self::deletion(TextRange::empty(at))
        } else {
            Self {
                content: Some(Box::from(content)),
                range: TextRange::new(at, at),
            }
        }
    }

    /// Returns the new content for an insertion or replacement.
    ///
    /// When this edit is a deletion, then this returns `None`.
    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }

    /// Consumes this edit to give back an owned value of the new content for
    /// an insertion or replacement.
    ///
    /// When this edit is a deletion, then this returns `None`.
    pub fn into_content(self) -> Option<Box<str>> {
        self.content
    }

    /// Returns the range of this edit.
    pub fn range(&self) -> TextRange {
        self.range
    }
}
