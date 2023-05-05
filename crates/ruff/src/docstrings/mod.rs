use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::Expr;

use ruff_python_semantic::definition::Definition;

pub mod extraction;
pub mod google;
pub mod numpy;
pub mod sections;
pub mod styles;

#[derive(Debug)]
pub struct Docstring<'a> {
    pub definition: &'a Definition<'a>,
    pub expr: &'a Expr,
    /// The content of the docstring, including the leading and trailing quotes.
    pub contents: &'a str,

    /// The range of the docstring body (without the quotes). The range is relative to [`Self::contents`].
    pub body_range: TextRange,
    pub indentation: &'a str,
}

impl<'a> Docstring<'a> {
    pub fn body(&self) -> DocstringBody {
        DocstringBody { docstring: self }
    }

    pub const fn start(&self) -> TextSize {
        self.expr.start()
    }

    pub const fn end(&self) -> TextSize {
        self.expr.end()
    }

    pub const fn range(&self) -> TextRange {
        self.expr.range()
    }

    pub fn leading_quote(&self) -> &'a str {
        &self.contents[TextRange::up_to(self.body_range.start())]
    }
}

#[derive(Copy, Clone)]
pub struct DocstringBody<'a> {
    docstring: &'a Docstring<'a>,
}

impl<'a> DocstringBody<'a> {
    #[inline]
    pub fn start(self) -> TextSize {
        self.range().start()
    }

    #[inline]
    pub fn end(self) -> TextSize {
        self.range().end()
    }

    pub fn range(self) -> TextRange {
        self.docstring.body_range + self.docstring.start()
    }

    pub fn as_str(self) -> &'a str {
        &self.docstring.contents[self.docstring.body_range]
    }
}

impl Deref for DocstringBody<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Debug for DocstringBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocstringBody")
            .field("text", &self.as_str())
            .field("range", &self.range())
            .finish()
    }
}
