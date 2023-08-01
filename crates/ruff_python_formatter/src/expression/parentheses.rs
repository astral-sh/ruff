use ruff_python_ast::Ranged;

use ruff_formatter::prelude::tag::Condition;
use ruff_formatter::{format_args, write, Argument, Arguments};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_trivia::{first_non_trivia_token, SimpleToken, SimpleTokenKind, SimpleTokenizer};

use crate::context::{NodeLevel, WithNodeLevel};
use crate::prelude::*;

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

impl OptionalParentheses {
    pub(crate) const fn is_always(self) -> bool {
        matches!(self, OptionalParentheses::Always)
    }
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

    /// Only adds parentheses if absolutely necessary:
    /// * The expression is not enclosed by another parenthesized expression and it expands over multiple lines
    /// * The expression has leading or trailing comments. Adding parentheses is desired to prevent the comments from wandering.
    IfRequired,
}

impl Parenthesize {
    pub(crate) const fn is_optional(self) -> bool {
        matches!(self, Parenthesize::Optional)
    }
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
    // First test if there's a closing parentheses because it tends to be cheaper.
    if matches!(
        first_non_trivia_token(expr.end(), contents),
        Some(SimpleToken {
            kind: SimpleTokenKind::RParen,
            ..
        })
    ) {
        let mut tokenizer =
            SimpleTokenizer::up_to_without_back_comment(expr.start(), contents).skip_trivia();

        matches!(
            tokenizer.next_back(),
            Some(SimpleToken {
                kind: SimpleTokenKind::LParen,
                ..
            })
        )
    } else {
        false
    }
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

        let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);

        if let NodeLevel::Expression(Some(group_id)) = current_level {
            // Use fits expanded if there's an enclosing group that adds the optional parentheses.
            // This ensures that expanding this parenthesized expression does not expand the optional parentheses group.
            write!(
                f,
                [fits_expanded(&inner)
                    .with_condition(Some(Condition::if_group_fits_on_line(group_id)))]
            )
        } else {
            // It's not necessary to wrap the content if it is not inside of an optional_parentheses group.
            write!(f, [inner])
        }
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
        // The group id is used as a condition in [`in_parentheses_only`] to create a conditional group
        // that is only active if the optional parentheses group expands.
        let parens_id = f.group_id("optional_parentheses");

        let mut f = WithNodeLevel::new(NodeLevel::Expression(Some(parens_id)), f);

        // We can't use `soft_block_indent` here because that would always increment the indent,
        // even if the group does not break (the indent is not soft). This would result in
        // too deep indentations if a `parenthesized` group expands. Using `indent_if_group_breaks`
        // gives us the desired *soft* indentation that is only present if the optional parentheses
        // are shown.
        write!(
            f,
            [group(&format_args![
                if_group_breaks(&text("(")),
                indent_if_group_breaks(
                    &format_args![soft_line_break(), Arguments::from(&self.content)],
                    parens_id
                ),
                soft_line_break(),
                if_group_breaks(&text(")"))
            ])
            .with_group_id(Some(parens_id))]
        )
    }
}

/// Creates a [`soft_line_break`] if the expression is enclosed by (optional) parentheses (`()`, `[]`, or `{}`).
/// Prints nothing if the expression is not parenthesized.
pub(crate) const fn in_parentheses_only_soft_line_break() -> InParenthesesOnlyLineBreak {
    InParenthesesOnlyLineBreak::SoftLineBreak
}

/// Creates a [`soft_line_break_or_space`] if the expression is enclosed by (optional) parentheses (`()`, `[]`, or `{}`).
/// Prints a [`space`] if the expression is not parenthesized.
pub(crate) const fn in_parentheses_only_soft_line_break_or_space() -> InParenthesesOnlyLineBreak {
    InParenthesesOnlyLineBreak::SoftLineBreakOrSpace
}

pub(crate) enum InParenthesesOnlyLineBreak {
    SoftLineBreak,
    SoftLineBreakOrSpace,
}

impl<'ast> Format<PyFormatContext<'ast>> for InParenthesesOnlyLineBreak {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        match f.context().node_level() {
            NodeLevel::TopLevel | NodeLevel::CompoundStatement | NodeLevel::Expression(None) => {
                match self {
                    InParenthesesOnlyLineBreak::SoftLineBreak => Ok(()),
                    InParenthesesOnlyLineBreak::SoftLineBreakOrSpace => space().fmt(f),
                }
            }
            NodeLevel::Expression(Some(parentheses_id)) => match self {
                InParenthesesOnlyLineBreak::SoftLineBreak => if_group_breaks(&soft_line_break())
                    .with_group_id(Some(parentheses_id))
                    .fmt(f),
                InParenthesesOnlyLineBreak::SoftLineBreakOrSpace => write!(
                    f,
                    [
                        if_group_breaks(&soft_line_break_or_space())
                            .with_group_id(Some(parentheses_id)),
                        if_group_fits_on_line(&space()).with_group_id(Some(parentheses_id))
                    ]
                ),
            },
            NodeLevel::ParenthesizedExpression => {
                f.write_element(FormatElement::Line(match self {
                    InParenthesesOnlyLineBreak::SoftLineBreak => LineMode::Soft,
                    InParenthesesOnlyLineBreak::SoftLineBreakOrSpace => LineMode::SoftOrSpace,
                }))
            }
        }
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
        match f.context().node_level() {
            NodeLevel::Expression(Some(parentheses_id)) => {
                // If this content is enclosed by a group that adds the optional parentheses, then *disable*
                // this group *except* if the optional parentheses are shown.
                conditional_group(
                    &Arguments::from(&self.content),
                    Condition::if_group_breaks(parentheses_id),
                )
                .fmt(f)
            }
            NodeLevel::ParenthesizedExpression => {
                // Unconditionally group the content if it is not enclosed by an optional parentheses group.
                group(&Arguments::from(&self.content)).fmt(f)
            }
            NodeLevel::Expression(None) | NodeLevel::TopLevel | NodeLevel::CompoundStatement => {
                Arguments::from(&self.content).fmt(f)
            }
        }
    }
}
