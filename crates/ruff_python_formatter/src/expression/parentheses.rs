use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::{first_non_trivia_token, first_non_trivia_token_rev, Token, TokenKind};
use ruff_formatter::prelude::tag::Condition;
use ruff_formatter::{format_args, Argument, Arguments};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::Ranged;

pub(crate) trait NeedsParentheses {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        context: &PyFormatContext,
    ) -> Parentheses;
}

pub(super) fn default_expression_needs_parentheses(
    node: AnyNodeRef,
    parenthesize: Parenthesize,
    context: &PyFormatContext,
) -> Parentheses {
    debug_assert!(
        node.is_expression(),
        "Should only be called for expressions"
    );

    #[allow(clippy::if_same_then_else)]
    if parenthesize.is_always() {
        Parentheses::Always
    } else if parenthesize.is_never() {
        Parentheses::Never
    }
    // `Optional` or `Preserve` and expression has parentheses in source code.
    else if !parenthesize.is_if_breaks() && is_expression_parenthesized(node, context.source()) {
        Parentheses::Always
    }
    // `Optional` or `IfBreaks`: Add parentheses if the expression doesn't fit on a line but enforce
    // parentheses if the expression has leading comments
    else if !parenthesize.is_preserve() {
        if context.comments().has_leading_comments(node) {
            Parentheses::Always
        } else {
            Parentheses::Optional
        }
    } else {
        //`Preserve` and expression has no parentheses in the source code
        Parentheses::Never
    }
}

/// Configures if the expression should be parenthesized.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum Parenthesize {
    /// Parenthesize the expression if it has parenthesis in the source.
    #[default]
    Preserve,

    /// Parenthesizes the expression if it doesn't fit on a line OR if the expression is parenthesized in the source code.
    Optional,

    /// Parenthesizes the expression only if it doesn't fit on a line.
    IfBreaks,

    /// Always adds parentheses
    Always,

    /// Never adds parentheses. Parentheses are handled by the caller.
    Never,
}

impl Parenthesize {
    pub(crate) const fn is_always(self) -> bool {
        matches!(self, Parenthesize::Always)
    }

    pub(crate) const fn is_never(self) -> bool {
        matches!(self, Parenthesize::Never)
    }

    pub(crate) const fn is_if_breaks(self) -> bool {
        matches!(self, Parenthesize::IfBreaks)
    }

    pub(crate) const fn is_preserve(self) -> bool {
        matches!(self, Parenthesize::Preserve)
    }
}

/// Whether it is necessary to add parentheses around an expression.
/// This is different from [`Parenthesize`] in that it is the resolved representation: It takes into account
/// whether there are parentheses in the source code or not.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Parentheses {
    /// Always set parentheses regardless if the expression breaks or if they were
    /// present in the source.
    Always,

    /// Only add parentheses when necessary because the expression breaks over multiple lines.
    Optional,

    /// Custom handling by the node's formatter implementation
    Custom,

    /// Never add parentheses
    Never,
}

pub(crate) fn is_expression_parenthesized(expr: AnyNodeRef, contents: &str) -> bool {
    matches!(
        first_non_trivia_token(expr.end(), contents),
        Some(Token {
            kind: TokenKind::RParen,
            ..
        })
    ) && matches!(
        first_non_trivia_token_rev(expr.start(), contents),
        Some(Token {
            kind: TokenKind::LParen,
            ..
        })
    )
}

/// Formats `content` enclosed by the `left` and `right` parentheses. The implementation also ensures
/// that expanding the parenthesized expression (or any of its children) doesn't enforce the
/// optional parentheses around the outer-most expression to materialize.
pub(crate) fn parenthesized<'content, 'ast, Content>(
    left: &'static str,
    content: &'content Content,
    right: &'static str,
) -> FormatParenthesized<'content, 'ast>
where
    Content: Format<PyFormatContext<'ast>>,
{
    FormatParenthesized {
        left,
        content: Argument::new(content),
        right,
    }
}

pub(crate) struct FormatParenthesized<'content, 'ast> {
    left: &'static str,
    content: Argument<'content, PyFormatContext<'ast>>,
    right: &'static str,
}

impl<'ast> Format<PyFormatContext<'ast>> for FormatParenthesized<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        let inner = format_with(|f| {
            group(&format_args![
                text(self.left),
                &soft_block_indent(&Arguments::from(&self.content)),
                text(self.right)
            ])
            .fmt(f)
        });

        let current_level = f.context().node_level();

        f.context_mut()
            .set_node_level(NodeLevel::ParenthesizedExpression);

        let result = if let NodeLevel::Expression(Some(group_id)) = current_level {
            // Use fits expanded if there's an enclosing group that adds the optional parentheses.
            // This ensures that expanding this parenthesized expression does not expand the optional parentheses group.
            fits_expanded(&inner)
                .with_condition(Some(Condition::if_group_fits_on_line(group_id)))
                .fmt(f)
        } else {
            // It's not necessary to wrap the content if it is not inside of an optional_parentheses group.
            inner.fmt(f)
        };

        f.context_mut().set_node_level(current_level);

        result
    }
}

/// Wraps an expression in parentheses only if it still does not fit after expanding all expressions that start or end with
/// a parentheses (`()`, `[]`, `{}`).
pub(crate) fn optional_parentheses<'content, 'ast, Content>(
    content: &'content Content,
) -> OptionalParentheses<'content, 'ast>
where
    Content: Format<PyFormatContext<'ast>>,
{
    OptionalParentheses {
        content: Argument::new(content),
    }
}

pub(crate) struct OptionalParentheses<'content, 'ast> {
    content: Argument<'content, PyFormatContext<'ast>>,
}

impl<'ast> Format<PyFormatContext<'ast>> for OptionalParentheses<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        let saved_level = f.context().node_level();

        // The group id is used as a condition in [`in_parentheses_only`] to create a conditional group
        // that is only active if the optional parentheses group expands.
        let parens_id = f.group_id("optional_parentheses");

        f.context_mut()
            .set_node_level(NodeLevel::Expression(Some(parens_id)));

        // We can't use `soft_block_indent` here because that would always increment the indent,
        // even if the group does not break (the indent is not soft). This would result in
        // too deep indentations if a `parenthesized` group expands. Using `indent_if_group_breaks`
        // gives us the desired *soft* indentation that is only present if the optional parentheses
        // are shown.
        let result = group(&format_args![
            if_group_breaks(&text("(")),
            indent_if_group_breaks(
                &format_args![soft_line_break(), Arguments::from(&self.content)],
                parens_id
            ),
            soft_line_break(),
            if_group_breaks(&text(")"))
        ])
        .with_group_id(Some(parens_id))
        .fmt(f);

        f.context_mut().set_node_level(saved_level);

        result
    }
}

/// Makes `content` a group, but only if the outer expression is parenthesized (a list, parenthesized expression, dict, ...)
/// or if the expression gets parenthesized because it expands over multiple lines.
pub(crate) fn in_parentheses_only_group<'content, 'ast, Content>(
    content: &'content Content,
) -> FormatInParenthesesOnlyGroup<'content, 'ast>
where
    Content: Format<PyFormatContext<'ast>>,
{
    FormatInParenthesesOnlyGroup {
        content: Argument::new(content),
    }
}

pub(crate) struct FormatInParenthesesOnlyGroup<'content, 'ast> {
    content: Argument<'content, PyFormatContext<'ast>>,
}

impl<'ast> Format<PyFormatContext<'ast>> for FormatInParenthesesOnlyGroup<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        if let NodeLevel::Expression(Some(group_id)) = f.context().node_level() {
            // If this content is enclosed by a group that adds the optional parentheses, then *disable*
            // this group *except* if the optional parentheses are shown.
            conditional_group(
                &Arguments::from(&self.content),
                Condition::if_group_breaks(group_id),
            )
            .fmt(f)
        } else {
            // Unconditionally group the content if it is not enclosed by an optional parentheses group.
            group(&Arguments::from(&self.content)).fmt(f)
        }
    }
}
