use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use ruff_python_ast::{self as ast, StringFlags};
use ruff_python_semantic::Definition;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

pub(crate) mod extraction;
pub(crate) mod google;
pub(crate) mod numpy;
pub(crate) mod sections;
pub(crate) mod styles;

#[derive(Debug)]
pub(crate) struct Docstring<'a> {
    pub(crate) definition: &'a Definition<'a>,
    /// The literal AST node representing the docstring.
    pub(crate) expr: &'a ast::StringLiteral,
    /// The source file the docstring was defined in.
    pub(crate) source: &'a str,
}

impl<'a> Docstring<'a> {
    fn flags(&self) -> ast::StringLiteralFlags {
        self.expr.flags
    }

    /// The contents of the docstring, including the opening and closing quotes.
    pub(crate) fn contents(&self) -> &'a str {
        &self.source[self.range()]
    }

    /// The contents of the docstring, excluding the opening and closing quotes.
    pub(crate) fn body(&self) -> DocstringBody {
        DocstringBody { docstring: self }
    }

    pub(crate) fn indentation(&self) -> &'a str {
        &self.source[TextRange::new(self.source.line_start(self.start()), self.start())]
    }

    pub(crate) fn quote_style(&self) -> ast::str::Quote {
        self.flags().quote_style()
    }

    pub(crate) fn is_raw_string(&self) -> bool {
        self.flags().prefix().is_raw()
    }

    pub(crate) fn is_u_string(&self) -> bool {
        self.flags().prefix().is_unicode()
    }

    pub(crate) fn is_triple_quoted(&self) -> bool {
        self.flags().is_triple_quoted()
    }

    pub(crate) fn prefixes(&self) -> &'a str {
        &self.source[TextRange::new(
            self.start(),
            self.start() + self.flags().prefix().text_len(),
        )]
    }

    /// The docstring's "opener" (the string's prefix, if any, and its opening quotes).
    pub(crate) fn opener(&self) -> &'a str {
        &self.source[TextRange::new(self.start(), self.start() + self.flags().opener_len())]
    }

    /// The docstring's closing quotes.
    pub(crate) fn closer(&self) -> &'a str {
        &self.source[TextRange::new(self.end() - self.flags().closer_len(), self.end())]
    }
}

impl Ranged for Docstring<'_> {
    fn range(&self) -> TextRange {
        self.expr.range()
    }
}

#[derive(Copy, Clone)]
pub(crate) struct DocstringBody<'a> {
    docstring: &'a Docstring<'a>,
}

impl<'a> DocstringBody<'a> {
    pub(crate) fn as_str(self) -> &'a str {
        &self.docstring.source[self.range()]
    }
}

impl Ranged for DocstringBody<'_> {
    fn range(&self) -> TextRange {
        TextRange::new(
            self.docstring.start() + self.docstring.flags().opener_len(),
            self.docstring.end() - self.docstring.flags().closer_len(),
        )
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
