use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::{first_non_trivia_token, first_non_trivia_token_rev, Token, TokenKind};
use ruff_formatter::prelude::tag::Condition;
use ruff_formatter::{format_args, Argument, Arguments};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::Ranged;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum OptionalParentheses {
    /// Add parentheses if the expression expands over multiple lines
    Multiline,

    /// Always set parentheses regardless if the expression breaks or if they were
    /// present in the source.
    Always,

    /// Never add parentheses
    Never,
}

pub(crate) trait NeedsParentheses {
    /// Determines if this object needs optional parentheses or if it is safe to omit the parentheses.
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses;
}

/// Configures if the expression should be parenthesized.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum Parenthesize {
    /// Parenthesizes the expression if it doesn't fit on a line OR if the expression is parenthesized in the source code.
    Optional,

    /// Parenthesizes the expression only if it doesn't fit on a line.
    IfBreaks,
}

/// Whether it is necessary to add parentheses around an expression.
/// This is different from [`Parenthesize`] in that it is the resolved representation: It takes into account
/// whether there are parentheses in the source code or not.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum Parentheses {
    #[default]
    Preserve,

    /// Always set parentheses regardless if the expression breaks or if they were
    /// present in the source.
    Always,

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
) -> FormatOptionalParentheses<'content, 'ast>
where
    Content: Format<PyFormatContext<'ast>>,
{
    FormatOptionalParentheses {
        content: Argument::new(content),
    }
}

pub(crate) struct FormatOptionalParentheses<'content, 'ast> {
    content: Argument<'content, PyFormatContext<'ast>>,
}

impl<'ast> Format<PyFormatContext<'ast>> for FormatOptionalParentheses<'_, 'ast> {
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
