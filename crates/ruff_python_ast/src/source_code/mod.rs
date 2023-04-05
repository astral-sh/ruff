mod generator;
mod indexer;
mod line_index;
mod locator;
mod stylist;

pub use crate::source_code::line_index::{LineIndex, OneIndexed};
use crate::types::Range;
pub use generator::Generator;
pub use indexer::Indexer;
pub use locator::Locator;
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser as parser;
use rustpython_parser::ast::Location;
use rustpython_parser::{lexer, Mode, ParseError};

use std::sync::Arc;
pub use stylist::{LineEnding, Stylist};

/// Run round-trip source code generation on a given Python code.
pub fn round_trip(code: &str, source_path: &str) -> Result<String, ParseError> {
    let locator = Locator::new(code);
    let python_ast = parser::parse_program(code, source_path)?;
    let tokens: Vec<_> = lexer::lex(code, Mode::Module).collect();
    let stylist = Stylist::from_tokens(&tokens, &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(&python_ast);
    Ok(generator.generate())
}

/// Gives access to the source code of a file and allows mapping between [`Location`] and byte offsets.
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

    /// Take the source code up to the given [`Location`].
    pub fn up_to(&self, location: Location) -> &'src str {
        let offset = self.index.location_offset(location, self.text);
        &self.text[TextRange::up_to(offset)]
    }

    /// Take the source code after the given [`Location`].
    pub fn after(&self, location: Location) -> &'src str {
        let offset = self.index.location_offset(location, self.text);
        &self.text[usize::from(offset)..]
    }

    /// Take the source code between the given [`Range`].
    pub fn slice<R: Into<Range>>(&self, range: R) -> &'src str {
        let range = self.text_range(range);
        &self.text[range]
    }

    /// Converts a [`Location`] range to a byte offset range
    pub fn text_range<R: Into<Range>>(&self, range: R) -> TextRange {
        let range = range.into();
        let start = self.index.location_offset(range.location, self.text);
        let end = self.index.location_offset(range.end_location, self.text);
        TextRange::new(start, end)
    }

    /// Return the byte offset of the given [`Location`].
    pub fn offset(&self, location: Location) -> TextSize {
        self.index.location_offset(location, self.text)
    }

    pub fn line_start(&self, line: OneIndexed) -> TextSize {
        self.index.line_start(line, self.text)
    }

    pub fn line_range(&self, line: OneIndexed) -> TextRange {
        self.index.line_range(line, self.text)
    }

    /// Returns a string with the lines spawning between location and end location.
    pub fn lines(&self, range: Range) -> &'src str {
        let start_line = self
            .index
            .line_range(OneIndexed::new(range.location.row()).unwrap(), self.text);

        let end_line = self.index.line_range(
            OneIndexed::new(range.end_location.row()).unwrap(),
            self.text,
        );

        &self.text[TextRange::new(start_line.start(), end_line.end())]
    }

    /// Returns the source text of the line with the given index
    #[inline]
    pub fn line_text(&self, index: OneIndexed) -> &'src str {
        let range = self.index.line_range(index, self.text);
        &self.text[range]
    }

    pub fn text(&self) -> &'src str {
        self.text
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        self.index.line_count()
    }

    pub fn to_source_code_buf(&self) -> SourceCodeBuf {
        self.to_owned()
    }

    pub fn to_owned(&self) -> SourceCodeBuf {
        SourceCodeBuf::new(self.text, self.index.clone())
    }
}

impl PartialEq<Self> for SourceCode<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for SourceCode<'_, '_> {}

impl PartialEq<SourceCodeBuf> for SourceCode<'_, '_> {
    fn eq(&self, other: &SourceCodeBuf) -> bool {
        self.text == &*other.text
    }
}

/// Gives access to the source code of a file and allows mapping between [`Location`] and byte offsets.
///
/// This is the owned pendant to [`SourceCode`]. Cloning only requires bumping reference counters.
#[derive(Clone, Debug)]
pub struct SourceCodeBuf {
    text: Arc<str>,
    index: LineIndex,
}

impl SourceCodeBuf {
    pub fn new(content: &str, index: LineIndex) -> Self {
        Self {
            text: Arc::from(content),
            index,
        }
    }

    /// Creates the [`LineIndex`] for `text` and returns the [`SourceCodeBuf`].
    pub fn from_content(text: &str) -> Self {
        Self::new(text, LineIndex::from_source_text(text))
    }

    #[inline]
    fn as_source_code(&self) -> SourceCode {
        SourceCode {
            text: &self.text,
            index: &self.index,
        }
    }

    /// Take the source code up to the given [`Location`].
    pub fn up_to(&self, location: Location) -> &str {
        self.as_source_code().up_to(location)
    }

    /// Take the source code after the given [`Location`].
    pub fn after(&self, location: Location) -> &str {
        self.as_source_code().after(location)
    }

    /// Take the source code between the given [`Range`].
    #[inline]
    pub fn slice<R: Into<Range>>(&self, range: R) -> &str {
        self.as_source_code().slice(range)
    }

    /// Converts a [`Location`] range to a byte offset range
    #[inline]
    pub fn text_range<R: Into<Range>>(&self, range: R) -> TextRange {
        self.as_source_code().text_range(range)
    }

    #[inline]
    pub fn line_range(&self, line: OneIndexed) -> TextRange {
        self.as_source_code().line_range(line)
    }

    /// Return the byte offset of the given [`Location`].
    #[inline]
    pub fn offset(&self, location: Location) -> TextSize {
        self.as_source_code().offset(location)
    }

    #[inline]
    pub fn line_start(&self, line: OneIndexed) -> TextSize {
        self.as_source_code().line_start(line)
    }

    #[inline]
    pub fn lines(&self, range: Range) -> &str {
        self.as_source_code().lines(range)
    }

    /// Returns the source text of the line with the given index
    #[inline]
    pub fn line_text(&self, index: OneIndexed) -> &str {
        self.as_source_code().line_text(index)
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        self.index.line_count()
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl PartialEq<Self> for SourceCodeBuf {
    // The same source text should have the same index
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl PartialEq<SourceCode<'_, '_>> for SourceCodeBuf {
    fn eq(&self, other: &SourceCode<'_, '_>) -> bool {
        &*self.text == other.text
    }
}

impl Eq for SourceCodeBuf {}
