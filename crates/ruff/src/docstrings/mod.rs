use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{Expr, Ranged};

use ruff_python_semantic::Definition;

pub(crate) mod extraction;
pub(crate) mod google;
pub(crate) mod numpy;
pub(crate) mod sections;
pub(crate) mod styles;

#[derive(Debug)]
pub(crate) struct Docstring<'a> {
    pub(crate) definition: &'a Definition<'a>,
    pub(crate) expr: &'a Expr,
    /// The content of the docstring, including the leading and trailing quotes.
    pub(crate) contents: &'a str,
    /// The range of the docstring body (without the quotes). The range is relative to [`Self::contents`].
    pub(crate) body_range: TextRange,
    pub(crate) indentation: &'a str,
}

impl<'a> Docstring<'a> {
    pub(crate) fn body(&self) -> DocstringBody {
        DocstringBody { docstring: self }
    }

    pub(crate) fn start(&self) -> TextSize {
        self.expr.start()
    }

    pub(crate) fn end(&self) -> TextSize {
        self.expr.end()
    }

    pub(crate) fn range(&self) -> TextRange {
        self.expr.range()
    }

    pub(crate) fn leading_quote(&self) -> &'a str {
        &self.contents[TextRange::up_to(self.body_range.start())]
    }
}

#[derive(Copy, Clone)]
pub(crate) struct DocstringBody<'a> {
    docstring: &'a Docstring<'a>,
}

impl<'a> DocstringBody<'a> {
    #[inline]
    pub(crate) fn start(self) -> TextSize {
        self.range().start()
    }

    pub(crate) fn range(self) -> TextRange {
        self.docstring.body_range + self.docstring.start()
    }

    pub(crate) fn as_str(self) -> &'a str {
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
