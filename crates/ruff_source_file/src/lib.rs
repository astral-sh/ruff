use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use ruff_text_size::{Ranged, TextRange, TextSize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub mod line_index;
mod locator;
pub mod newlines;

pub use crate::line_index::{LineIndex, OneIndexed};
pub use locator::Locator;
pub use newlines::{
    find_newline, Line, LineEnding, NewlineWithTrailingNewline, UniversalNewlineIterator,
    UniversalNewlines,
};

/// Gives access to the source code of a file and allows mapping between [`TextSize`] and [`SourceLocation`].
#[derive(Debug)]
pub struct SourceCode<'src, 'index> {
    text: &'src str,
    index: &'index LineIndex,
}

impl<'src, 'index> SourceCode<'src, 'index> {
    pub fn new(content: &'src str, index: &'index LineIndex) -> Self {
        Self {
            text: content,
            index,
        }
    }

    /// Computes the one indexed row and column numbers for `offset`.
    #[inline]
    pub fn source_location(&self, offset: TextSize) -> SourceLocation {
        self.index.source_location(offset, self.text)
    }

    #[inline]
    pub fn line_index(&self, offset: TextSize) -> OneIndexed {
        self.index.line_index(offset)
    }

    /// Take the source code up to the given [`TextSize`].
    #[inline]
    pub fn up_to(&self, offset: TextSize) -> &'src str {
        &self.text[TextRange::up_to(offset)]
    }

    /// Take the source code after the given [`TextSize`].
    #[inline]
    pub fn after(&self, offset: TextSize) -> &'src str {
        &self.text[usize::from(offset)..]
    }

    /// Take the source code between the given [`TextRange`].
    pub fn slice<T: Ranged>(&self, ranged: T) -> &'src str {
        &self.text[ranged.range()]
    }

    pub fn line_start(&self, line: OneIndexed) -> TextSize {
        self.index.line_start(line, self.text)
    }

    pub fn line_end(&self, line: OneIndexed) -> TextSize {
        self.index.line_end(line, self.text)
    }

    pub fn line_end_exclusive(&self, line: OneIndexed) -> TextSize {
        self.index.line_end_exclusive(line, self.text)
    }

    pub fn line_range(&self, line: OneIndexed) -> TextRange {
        self.index.line_range(line, self.text)
    }

    /// Returns the source text of the line with the given index
    #[inline]
    pub fn line_text(&self, index: OneIndexed) -> &'src str {
        let range = self.index.line_range(index, self.text);
        &self.text[range]
    }

    /// Returns the source text
    pub fn text(&self) -> &'src str {
        self.text
    }

    /// Returns the number of lines
    #[inline]
    pub fn line_count(&self) -> usize {
        self.index.line_count()
    }
}

impl PartialEq<Self> for SourceCode<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for SourceCode<'_, '_> {}

/// A Builder for constructing a [`SourceFile`]
pub struct SourceFileBuilder {
    name: Box<str>,
    code: Box<str>,
    index: Option<LineIndex>,
}

impl SourceFileBuilder {
    /// Creates a new builder for a file named `name`.
    pub fn new<Name: Into<Box<str>>, Code: Into<Box<str>>>(name: Name, code: Code) -> Self {
        Self {
            name: name.into(),
            code: code.into(),
            index: None,
        }
    }

    #[must_use]
    pub fn line_index(mut self, index: LineIndex) -> Self {
        self.index = Some(index);
        self
    }

    pub fn set_line_index(&mut self, index: LineIndex) {
        self.index = Some(index);
    }

    /// Consumes `self` and returns the [`SourceFile`].
    pub fn finish(self) -> SourceFile {
        let index = if let Some(index) = self.index {
            once_cell::sync::OnceCell::with_value(index)
        } else {
            once_cell::sync::OnceCell::new()
        };

        SourceFile {
            inner: Arc::new(SourceFileInner {
                name: self.name,
                code: self.code,
                line_index: index,
            }),
        }
    }
}

/// A source file that is identified by its name. Optionally stores the source code and [`LineIndex`].
///
/// Cloning a [`SourceFile`] is cheap, because it only requires bumping a reference count.
#[derive(Clone, Eq, PartialEq)]
pub struct SourceFile {
    inner: Arc<SourceFileInner>,
}

impl Debug for SourceFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceFile")
            .field("name", &self.name())
            .field("code", &self.source_text())
            .finish()
    }
}

impl SourceFile {
    /// Returns the name of the source file (filename).
    #[inline]
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    #[inline]
    pub fn slice(&self, range: TextRange) -> &str {
        &self.source_text()[range]
    }

    pub fn to_source_code(&self) -> SourceCode {
        SourceCode {
            text: self.source_text(),
            index: self.index(),
        }
    }

    fn index(&self) -> &LineIndex {
        self.inner
            .line_index
            .get_or_init(|| LineIndex::from_source_text(self.source_text()))
    }

    /// Returns the source code.
    #[inline]
    pub fn source_text(&self) -> &str {
        &self.inner.code
    }
}

impl PartialOrd for SourceFile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SourceFile {
    fn cmp(&self, other: &Self) -> Ordering {
        // Short circuit if these are the same source files
        if Arc::ptr_eq(&self.inner, &other.inner) {
            Ordering::Equal
        } else {
            self.inner.name.cmp(&other.inner.name)
        }
    }
}

struct SourceFileInner {
    name: Box<str>,
    code: Box<str>,
    line_index: once_cell::sync::OnceCell<LineIndex>,
}

impl PartialEq for SourceFileInner {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.code == other.code
    }
}

impl Eq for SourceFileInner {}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SourceLocation {
    pub row: OneIndexed,
    pub column: OneIndexed,
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self {
            row: OneIndexed::MIN,
            column: OneIndexed::MIN,
        }
    }
}

impl Debug for SourceLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceLocation")
            .field("row", &self.row.get())
            .field("column", &self.column.get())
            .finish()
    }
}
