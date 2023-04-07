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
use std::fmt::{Debug, Formatter};

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

    pub fn line_end(&self, line: OneIndexed) -> TextSize {
        self.index.line_end(line, self.text)
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
    code: Option<FileSourceCode>,
}

impl SourceFileBuilder {
    /// Creates a new builder for a file named `name`.
    pub fn new(name: &str) -> Self {
        Self {
            name: Box::from(name),
            code: None,
        }
    }

    /// Creates a enw builder for a file named `name`
    pub fn from_string(name: String) -> Self {
        Self {
            name: Box::from(name),
            code: None,
        }
    }

    /// Consumes `self` and returns a builder for a file with the source text and the [`LineIndex`] copied
    /// from `source`.
    #[must_use]
    pub fn source_code(mut self, source: &SourceCode) -> Self {
        self.set_source_code(source);
        self
    }

    /// Copies the source text and [`LineIndex`] from `source`.
    pub fn set_source_code(&mut self, source: &SourceCode) {
        self.code = Some(FileSourceCode {
            text: Box::from(source.text()),
            index: source.index.clone(),
        });
    }

    /// Consumes `self` and returns a builder for a file with the source text `text`. Builds the [`LineIndex`] from `text`.
    #[must_use]
    pub fn source_text(self, text: &str) -> Self {
        self.source_code(&SourceCode::new(text, &LineIndex::from_source_text(text)))
    }

    /// Consumes `self` and returns a builder for a file with the source text `text`. Builds the [`LineIndex`] from `text`.
    #[must_use]
    pub fn source_text_string(mut self, text: String) -> Self {
        self.set_source_text_string(text);
        self
    }

    /// Copies the source text `text` and builds the [`LineIndex`] from `text`.
    pub fn set_source_text_string(&mut self, text: String) {
        self.code = Some(FileSourceCode {
            index: LineIndex::from_source_text(&text),
            text: Box::from(text),
        });
    }

    /// Consumes `self` and returns the [`SourceFile`].
    pub fn finish(self) -> SourceFile {
        SourceFile {
            inner: Arc::new(SourceFileInner {
                name: self.name,
                code: self.code,
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
            .field("code", &self.source_code())
            .finish()
    }
}

impl SourceFile {
    /// Returns the name of the source file (filename).
    #[inline]
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    /// Returns `Some` with the source code if set, or `None`.
    #[inline]
    pub fn source_code(&self) -> Option<SourceCode> {
        self.inner.code.as_ref().map(|code| SourceCode {
            text: &code.text,
            index: &code.index,
        })
    }

    /// Returns `Some` with the source text if set, or `None`.
    #[inline]
    pub fn source_text(&self) -> Option<&str> {
        self.inner.code.as_ref().map(|code| &*code.text)
    }
}

#[derive(Eq, PartialEq)]
struct SourceFileInner {
    name: Box<str>,
    code: Option<FileSourceCode>,
}

struct FileSourceCode {
    text: Box<str>,
    index: LineIndex,
}

impl PartialEq for FileSourceCode {
    fn eq(&self, other: &Self) -> bool {
        // It should be safe to assume that the index for two source files are identical
        self.text == other.text
    }
}

impl Eq for FileSourceCode {}
