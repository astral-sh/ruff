use crate::comments::Comments;
use crate::other::f_string_element::FStringExpressionElementContext;
use crate::PyFormatOptions;
use ruff_formatter::{Buffer, FormatContext, GroupId, IndentWidth, SourceCode};
use ruff_python_ast::str::Quote;
use ruff_python_parser::Tokens;
use ruff_source_file::Locator;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};

#[derive(Clone)]
pub struct PyFormatContext<'a> {
    options: PyFormatOptions,
    contents: &'a str,
    comments: Comments<'a>,
    tokens: &'a Tokens,
    node_level: NodeLevel,
    indent_level: IndentLevel,
    /// Set to a non-None value when the formatter is running on a code
    /// snippet within a docstring. The value should be the quote character of the
    /// docstring containing the code snippet.
    ///
    /// Various parts of the formatter may inspect this state to change how it
    /// works. For example, multi-line strings will always be written with a
    /// quote style that is inverted from the one here in order to ensure that
    /// the formatted Python code will be valid.
    docstring: Option<Quote>,
    /// The state of the formatter with respect to f-strings.
    f_string_state: FStringState,
}

impl<'a> PyFormatContext<'a> {
    pub(crate) fn new(
        options: PyFormatOptions,
        contents: &'a str,
        comments: Comments<'a>,
        tokens: &'a Tokens,
    ) -> Self {
        Self {
            options,
            contents,
            comments,
            tokens,
            node_level: NodeLevel::TopLevel(TopLevelStatementPosition::Other),
            indent_level: IndentLevel::new(0),
            docstring: None,
            f_string_state: FStringState::Outside,
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

    pub(crate) fn set_indent_level(&mut self, level: IndentLevel) {
        self.indent_level = level;
    }

    pub(crate) fn indent_level(&self) -> IndentLevel {
        self.indent_level
    }

    pub(crate) fn comments(&self) -> &Comments<'a> {
        &self.comments
    }

    pub(crate) fn tokens(&self) -> &'a Tokens {
        self.tokens
    }

    /// Returns a non-None value only if the formatter is running on a code
    /// snippet within a docstring.
    ///
    /// The quote character returned corresponds to the quoting used for the
    /// docstring containing the code snippet currently being formatted.
    pub(crate) fn docstring(&self) -> Option<Quote> {
        self.docstring
    }

    /// Return a new context suitable for formatting code snippets within a
    /// docstring.
    ///
    /// The quote character given should correspond to the quote character used
    /// for the docstring containing the code snippets.
    pub(crate) fn in_docstring(self, quote: Quote) -> PyFormatContext<'a> {
        PyFormatContext {
            docstring: Some(quote),
            ..self
        }
    }

    pub(crate) fn f_string_state(&self) -> FStringState {
        self.f_string_state
    }

    pub(crate) fn set_f_string_state(&mut self, f_string_state: FStringState) {
        self.f_string_state = f_string_state;
    }

    /// Returns `true` if preview mode is enabled.
    #[allow(unused)]
    pub(crate) const fn is_preview(&self) -> bool {
        self.options.preview().is_enabled()
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum FStringState {
    /// The formatter is inside an f-string expression element i.e., between the
    /// curly brace in `f"foo {x}"`.
    ///
    /// The containing `FStringContext` is the surrounding f-string context.
    InsideExpressionElement(FStringExpressionElementContext),
    /// The formatter is outside an f-string.
    #[default]
    Outside,
}

/// The position of a top-level statement in the module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub(crate) enum TopLevelStatementPosition {
    /// This is the last top-level statement in the module.
    Last,
    /// Any other top-level statement.
    #[default]
    Other,
}

/// What's the enclosing level of the outer node.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum NodeLevel {
    /// Formatting statements on the module level.
    TopLevel(TopLevelStatementPosition),

    /// Formatting the body statements of a [compound statement](https://docs.python.org/3/reference/compound_stmts.html#compound-statements)
    /// (`if`, `while`, `match`, etc.).
    CompoundStatement,

    /// The root or any sub-expression.
    Expression(Option<GroupId>),

    /// Formatting nodes that are enclosed by a parenthesized (any `[]`, `{}` or `()`) expression.
    ParenthesizedExpression,
}

impl Default for NodeLevel {
    fn default() -> Self {
        Self::TopLevel(TopLevelStatementPosition::Other)
    }
}

impl NodeLevel {
    /// Returns `true` if the expression is in a parenthesized context.
    pub(crate) const fn is_parenthesized(self) -> bool {
        matches!(
            self,
            NodeLevel::Expression(Some(_)) | NodeLevel::ParenthesizedExpression
        )
    }

    /// Returns `true` if this is the last top-level statement in the module.
    pub(crate) const fn is_last_top_level_statement(self) -> bool {
        matches!(self, NodeLevel::TopLevel(TopLevelStatementPosition::Last))
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

/// The current indent level of the formatter.
///
/// One can determine the width of the indent itself (in number of ASCII
/// space characters) by multiplying the indent level by the configured indent
/// width.
///
/// This is specifically used inside the docstring code formatter for
/// implementing its "dynamic" line width mode. Namely, in the nested call to
/// the formatter, when "dynamic" mode is enabled, the line width is set to
/// `min(1, line_width - indent_level * indent_width)`, where `line_width` in
/// this context is the global line width setting.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndentLevel {
    /// The numeric level. It is incremented for every whole indent in Python
    /// source code.
    ///
    /// Note that the first indentation level is actually 1, since this starts
    /// at 0 and is incremented when the first top-level statement is seen. So
    /// even though the first top-level statement in Python source will have no
    /// indentation, its indentation level is 1.
    level: u16,
}

impl IndentLevel {
    /// Returns a new indent level for the given value.
    pub(crate) fn new(level: u16) -> IndentLevel {
        IndentLevel { level }
    }

    /// Returns the next indent level.
    pub(crate) fn increment(self) -> IndentLevel {
        IndentLevel {
            level: self.level.saturating_add(1),
        }
    }

    /// Convert this indent level into a specific number of ASCII whitespace
    /// characters based on the given indent width.
    pub(crate) fn to_ascii_spaces(self, width: IndentWidth) -> u16 {
        let width = u16::try_from(width.value()).unwrap_or(u16::MAX);
        // Why the subtraction? IndentLevel starts at 0 and asks for the "next"
        // indent level before seeing the first top-level statement. So it's
        // always 1 more than what we expect it to be.
        let level = self.level.saturating_sub(1);
        width.saturating_mul(level)
    }
}

/// Change the [`IndentLevel`] of the formatter for the lifetime of this
/// struct.
pub(crate) struct WithIndentLevel<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    buffer: D,
    saved_level: IndentLevel,
}

impl<'a, B, D> WithIndentLevel<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    pub(crate) fn new(level: IndentLevel, mut buffer: D) -> Self {
        let context = buffer.state_mut().context_mut();
        let saved_level = context.indent_level();

        context.set_indent_level(level);

        Self {
            buffer,
            saved_level,
        }
    }
}

impl<'a, B, D> Deref for WithIndentLevel<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    type Target = B;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl<'a, B, D> DerefMut for WithIndentLevel<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl<'a, B, D> Drop for WithIndentLevel<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    fn drop(&mut self) {
        self.buffer
            .state_mut()
            .context_mut()
            .set_indent_level(self.saved_level);
    }
}

pub(crate) struct WithFStringState<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    buffer: D,
    saved_location: FStringState,
}

impl<'a, B, D> WithFStringState<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    pub(crate) fn new(expr_location: FStringState, mut buffer: D) -> Self {
        let context = buffer.state_mut().context_mut();
        let saved_location = context.f_string_state();

        context.set_f_string_state(expr_location);

        Self {
            buffer,
            saved_location,
        }
    }
}

impl<'a, B, D> Deref for WithFStringState<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    type Target = B;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl<'a, B, D> DerefMut for WithFStringState<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl<'a, B, D> Drop for WithFStringState<'a, B, D>
where
    D: DerefMut<Target = B>,
    B: Buffer<Context = PyFormatContext<'a>>,
{
    fn drop(&mut self) {
        self.buffer
            .state_mut()
            .context_mut()
            .set_f_string_state(self.saved_location);
    }
}
