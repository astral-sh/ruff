use crate::comments::Comments;
use crate::PyFormatOptions;
use ruff_formatter::{Buffer, FormatContext, GroupId, SourceCode};
use ruff_source_file::Locator;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};

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

    pub(crate) fn source(&self) -> &'a str {
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

    /// The root or any sub-expression.
    Expression(Option<GroupId>),

    /// Formatting nodes that are enclosed by a parenthesized (any `[]`, `{}` or `()`) expression.
    ParenthesizedExpression,
}

impl NodeLevel {
    /// Returns `true` if the expression is in a parenthesized context.
    pub(crate) const fn is_parenthesized(self) -> bool {
        matches!(
            self,
            NodeLevel::Expression(Some(_)) | NodeLevel::ParenthesizedExpression
        )
    }
}

/// Change the [`NodeLevel`] of the formatter for the lifetime of this struct
pub(crate) struct WithNodeLevel<'ast, 'buf, B>
where
    B: Buffer<Context = PyFormatContext<'ast>>,
{
    buffer: &'buf mut B,
    saved_level: NodeLevel,
}

impl<'ast, 'buf, B> WithNodeLevel<'ast, 'buf, B>
where
    B: Buffer<Context = PyFormatContext<'ast>>,
{
    pub(crate) fn new(level: NodeLevel, buffer: &'buf mut B) -> Self {
        let context = buffer.state_mut().context_mut();
        let saved_level = context.node_level();

        context.set_node_level(level);

        Self {
            buffer,
            saved_level,
        }
    }
}

impl<'ast, 'buf, B> Deref for WithNodeLevel<'ast, 'buf, B>
where
    B: Buffer<Context = PyFormatContext<'ast>>,
{
    type Target = B;

    fn deref(&self) -> &Self::Target {
        self.buffer
    }
}

impl<'ast, 'buf, B> DerefMut for WithNodeLevel<'ast, 'buf, B>
where
    B: Buffer<Context = PyFormatContext<'ast>>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer
    }
}

impl<'ast, B> Drop for WithNodeLevel<'ast, '_, B>
where
    B: Buffer<Context = PyFormatContext<'ast>>,
{
    fn drop(&mut self) {
        self.buffer
            .state_mut()
            .context_mut()
            .set_node_level(self.saved_level);
    }
}
