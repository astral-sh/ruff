use crate::comments::Comments;
use crate::PyFormatOptions;
use ruff_formatter::{FormatContext, SourceCode};
use ruff_python_ast::source_code::Locator;
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub struct PyFormatContext<'a> {
    options: PyFormatOptions,
    contents: &'a str,
    comments: Comments<'a>,
    node_level: NodeLevel,
}

impl<'a> PyFormatContext<'a> {
    pub(crate) fn new(options: PyFormatOptions, contents: &'a str, comments: Comments<'a>) -> Self {
        Self {
            options,
            contents,
            comments,
            node_level: NodeLevel::TopLevel,
        }
    }

    pub(crate) fn contents(&self) -> &'a str {
        self.contents
    }

    #[allow(unused)]
    pub(crate) fn locator(&self) -> Locator<'a> {
        Locator::new(self.contents)
    }

    pub(crate) fn set_node_level(&mut self, level: NodeLevel) {
        self.node_level = level;
    }

    pub(crate) fn node_level(&self) -> NodeLevel {
        self.node_level
    }

    pub(crate) fn comments(&self) -> &Comments<'a> {
        &self.comments
    }
}

impl FormatContext for PyFormatContext<'_> {
    type Options = PyFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }

    fn source_code(&self) -> SourceCode {
        SourceCode::new(self.contents)
    }
}

impl Debug for PyFormatContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyFormatContext")
            .field("options", &self.options)
            .field("comments", &self.comments.debug(self.source_code()))
            .field("node_level", &self.node_level)
            .field("source", &self.contents)
            .finish()
    }
}

/// What's the enclosing level of the outer node.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub(crate) enum NodeLevel {
    /// Formatting statements on the module level.
    #[default]
    TopLevel,

    /// Formatting the body statements of a [compound statement](https://docs.python.org/3/reference/compound_stmts.html#compound-statements)
    /// (`if`, `while`, `match`, etc.).
    CompoundStatement,

    /// Formatting nodes that are enclosed in a parenthesized expression.
    Expression,
}
