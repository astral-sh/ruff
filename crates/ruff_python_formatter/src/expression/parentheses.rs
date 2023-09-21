use ruff_formatter::prelude::tag::Condition;
use ruff_formatter::{format_args, write, Argument, Arguments};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExpressionRef;
use ruff_python_trivia::CommentRanges;
use ruff_python_trivia::{
    first_non_trivia_token, BackwardsTokenizer, SimpleToken, SimpleTokenKind,
};
use ruff_text_size::Ranged;

use crate::comments::{
    dangling_comments, dangling_open_parenthesis_comments, trailing_comments, SourceComment,
};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::prelude::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum OptionalParentheses {
    /// Add parentheses if the expression expands over multiple lines
    Multiline,

    /// Always set parentheses regardless if the expression breaks or if they are
    /// present in the source.
    Always,

    /// Add parentheses if it helps to make this expression fit. Otherwise never add parentheses.
    /// This mode should only be used for expressions that don't have their own split points to the left, e.g. identifiers,
    /// or constants, calls starting with an identifier, etc.
    BestFit,

    /// Never add parentheses. Use it for expressions that have their own parentheses or if the expression body always spans multiple lines (multiline strings).
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

    /// Only adds parentheses if the expression has leading or trailing comments.
    /// Adding parentheses is desired to prevent the comments from wandering.
    IfRequired,

    /// Parenthesizes the expression if the group doesn't fit on a line (e.g., even name expressions are parenthesized), or if
    /// the expression doesn't break, but _does_ reports that it always requires parentheses in this position (e.g., walrus
    /// operators in function return annotations).
    IfBreaksOrIfRequired,
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

pub(crate) fn is_expression_parenthesized(
    expr: ExpressionRef,
    comment_ranges: &CommentRanges,
    contents: &str,
) -> bool {
    // First test if there's a closing parentheses because it tends to be cheaper.
    if matches!(
        first_non_trivia_token(expr.end(), contents),
        Some(SimpleToken {
            kind: SimpleTokenKind::RParen,
            ..
        })
    ) {
        matches!(
            BackwardsTokenizer::up_to(expr.start(), contents, comment_ranges)
                .skip_trivia()
                .next(),
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
        comments: &[],
        content: Argument::new(content),
        right,
    }
}

pub(crate) struct FormatParenthesized<'content, 'ast> {
    left: &'static str,
    comments: &'content [SourceComment],
    content: Argument<'content, PyFormatContext<'ast>>,
    right: &'static str,
}

impl<'content, 'ast> FormatParenthesized<'content, 'ast> {
    /// Inserts any dangling comments that should be placed immediately after the open parenthesis.
    /// For example:
    /// ```python
    /// [  # comment
    ///     1,
    ///     2,
    ///     3,
    /// ]
    /// ```
    pub(crate) fn with_dangling_comments(
        self,
        comments: &'content [SourceComment],
    ) -> FormatParenthesized<'content, 'ast> {
        FormatParenthesized { comments, ..self }
    }
}

impl<'ast> Format<PyFormatContext<'ast>> for FormatParenthesized<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        let current_level = f.context().node_level();

        let content = format_with(|f| {
            group(&format_args![
                dangling_open_parenthesis_comments(self.comments),
                soft_block_indent(&Arguments::from(&self.content))
            ])
            .fmt(f)
        });

        let inner = format_with(|f| {
            if let NodeLevel::Expression(Some(group_id)) = current_level {
                // Use fits expanded if there's an enclosing group that adds the optional parentheses.
                // This ensures that expanding this parenthesized expression does not expand the optional parentheses group.
                write!(
                    f,
                    [fits_expanded(&content)
                        .with_condition(Some(Condition::if_group_fits_on_line(group_id)))]
                )
            } else {
                // It's not necessary to wrap the content if it is not inside of an optional_parentheses group.
                content.fmt(f)
            }
        });

        let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);

        write!(f, [token(self.left), inner, token(self.right)])
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
        // The group id is used as a condition in [`in_parentheses_only_group`] to create a
        // conditional group that is only active if the optional parentheses group expands.
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
                if_group_breaks(&token("(")),
                indent_if_group_breaks(
                    &format_args![soft_line_break(), Arguments::from(&self.content)],
                    parens_id
                ),
                soft_line_break(),
                if_group_breaks(&token(")"))
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
                }));
                Ok(())
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
        write_in_parentheses_only_group_start_tag(f);
        Arguments::from(&self.content).fmt(f)?;
        write_in_parentheses_only_group_end_tag(f);
        Ok(())
    }
}

pub(super) fn write_in_parentheses_only_group_start_tag(f: &mut PyFormatter) {
    match f.context().node_level() {
        NodeLevel::Expression(Some(parentheses_id)) => {
            f.write_element(FormatElement::Tag(tag::Tag::StartConditionalGroup(
                tag::ConditionalGroup::new(Condition::if_group_breaks(parentheses_id)),
            )));
        }
        NodeLevel::ParenthesizedExpression => {
            // Unconditionally group the content if it is not enclosed by an optional parentheses group.
            f.write_element(FormatElement::Tag(tag::Tag::StartGroup(tag::Group::new())));
        }
        NodeLevel::Expression(None) | NodeLevel::TopLevel | NodeLevel::CompoundStatement => {
            // No group
        }
    }
}

pub(super) fn write_in_parentheses_only_group_end_tag(f: &mut PyFormatter) {
    match f.context().node_level() {
        NodeLevel::Expression(Some(_)) => {
            f.write_element(FormatElement::Tag(tag::Tag::EndConditionalGroup));
        }
        NodeLevel::ParenthesizedExpression => {
            // Unconditionally group the content if it is not enclosed by an optional parentheses group.
            f.write_element(FormatElement::Tag(tag::Tag::EndGroup));
        }
        NodeLevel::Expression(None) | NodeLevel::TopLevel | NodeLevel::CompoundStatement => {
            // No group
        }
    }
}

/// Format comments inside empty parentheses, brackets or curly braces.
///
/// Empty `()`, `[]` and `{}` are special because there can be dangling comments, and they can be in
/// two positions:
/// ```python
/// x = [  # end-of-line
///     # own line
/// ]
/// ```
/// These comments are dangling because they can't be assigned to any element inside as they would
/// in all other cases.
pub(crate) fn empty_parenthesized<'content>(
    left: &'static str,
    comments: &'content [SourceComment],
    right: &'static str,
) -> FormatEmptyParenthesized<'content> {
    FormatEmptyParenthesized {
        left,
        comments,
        right,
    }
}

pub(crate) struct FormatEmptyParenthesized<'content> {
    left: &'static str,
    comments: &'content [SourceComment],
    right: &'static str,
}

impl Format<PyFormatContext<'_>> for FormatEmptyParenthesized<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        let end_of_line_split = self
            .comments
            .partition_point(|comment| comment.line_position().is_end_of_line());
        debug_assert!(self.comments[end_of_line_split..]
            .iter()
            .all(|comment| comment.line_position().is_own_line()));
        write!(
            f,
            [group(&format_args![
                token(self.left),
                // end-of-line comments
                trailing_comments(&self.comments[..end_of_line_split]),
                // Avoid unstable formatting with
                // ```python
                // x = () - (#
                // )
                // ```
                // Without this the comment would go after the empty tuple first, but still expand
                // the bin op. In the second formatting pass they are trailing bin op comments
                // so the bin op collapse. Suboptimally we keep parentheses around the bin op in
                // either case.
                (!self.comments[..end_of_line_split].is_empty()).then_some(hard_line_break()),
                // own line comments, which need to be indented
                soft_block_indent(&dangling_comments(&self.comments[end_of_line_split..])),
                token(self.right)
            ])]
        )
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::ExpressionRef;
    use ruff_python_parser::parse_expression;
    use ruff_python_trivia::CommentRanges;

    use crate::expression::parentheses::is_expression_parenthesized;

    #[test]
    fn test_has_parentheses() {
        let expression = r#"(b().c("")).d()"#;
        let expr = parse_expression(expression, "<filename>").unwrap();
        assert!(!is_expression_parenthesized(
            ExpressionRef::from(&expr),
            &CommentRanges::default(),
            expression
        ));
    }
}
