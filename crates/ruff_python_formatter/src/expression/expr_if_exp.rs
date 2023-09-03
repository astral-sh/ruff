use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Expr, ExprIfExp, ExpressionRef};
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::Ranged;

use crate::comments::leading_comments;
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break_or_space,
    is_expression_parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default, Copy, Clone)]
pub enum ExprIfExpLayout {
    #[default]
    Default,

    /// The [`ExprIfExp`] is nested inside another [`ExprIfExp`], so it should not be given a new
    /// group. For example, avoid grouping the `else` clause in:
    /// ```python
    /// clone._iterable_class = (
    ///     NamedValuesListIterable
    ///     if named
    ///     else FlatValuesListIterable
    ///     if flat
    ///     else ValuesListIterable
    /// )
    /// ```
    Nested,
}

#[derive(Default)]
pub struct FormatExprIfExp {
    layout: ExprIfExpLayout,
}

impl FormatRuleWithOptions<ExprIfExp, PyFormatContext<'_>> for FormatExprIfExp {
    type Options = ExprIfExpLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
    }
}

impl FormatNodeRule<ExprIfExp> for FormatExprIfExp {
    fn fmt_fields(&self, item: &ExprIfExp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprIfExp {
            range: _,
            test,
            body,
            orelse,
        } = item;
        let comments = f.context().comments().clone();

        let inner = format_with(|f: &mut PyFormatter| {
            // If the expression has any leading or trailing comments, and is "alone" in brackets,
            // always expand it, as in:
            // ```
            // [
            //     # comment
            //     0
            //     if self.thing is None
            //     else before - after
            // ]
            // ```
            if (comments.has_leading(item) || comments.has_trailing(item))
                && is_expression_bracketed(item.into(), f.context().source())
            {
                expand_parent().fmt(f)?;
            }

            // We place `if test` and `else orelse` on a single line, so the `test` and `orelse`
            // leading comments go on the line before the `if` or `else` instead of directly ahead
            // `test` or `orelse`.
            write!(
                f,
                [
                    body.format(),
                    in_parentheses_only_soft_line_break_or_space(),
                    leading_comments(comments.leading(test.as_ref())),
                    token("if"),
                    space(),
                    test.format(),
                    in_parentheses_only_soft_line_break_or_space(),
                    leading_comments(comments.leading(orelse.as_ref())),
                    token("else"),
                    space(),
                ]
            )?;

            FormatOrElse { orelse }.fmt(f)
        });

        match self.layout {
            ExprIfExpLayout::Default => in_parentheses_only_group(&inner).fmt(f),
            ExprIfExpLayout::Nested => inner.fmt(f),
        }
    }
}

impl NeedsParentheses for ExprIfExp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}

#[derive(Debug)]
struct FormatOrElse<'a> {
    orelse: &'a Expr,
}

impl Format<PyFormatContext<'_>> for FormatOrElse<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self.orelse {
            Expr::IfExp(expr)
                if !is_expression_parenthesized(expr.into(), f.context().source()) =>
            {
                write!(f, [expr.format().with_options(ExprIfExpLayout::Nested)])
            }
            _ => write!(f, [in_parentheses_only_group(&self.orelse.format())]),
        }
    }
}

/// Returns `true` if the expression is surrounded by brackets (e.g., parenthesized, or the only
/// expression in a list, tuple, or set).
fn is_expression_bracketed(expr: ExpressionRef, contents: &str) -> bool {
    let mut tokenizer = SimpleTokenizer::starts_at(expr.end(), contents)
        .skip_trivia()
        .skip_while(|token| matches!(token.kind, SimpleTokenKind::Comma));

    if matches!(
        tokenizer.next(),
        Some(SimpleToken {
            kind: SimpleTokenKind::RParen | SimpleTokenKind::RBrace | SimpleTokenKind::RBracket,
            ..
        })
    ) {
        let mut tokenizer =
            SimpleTokenizer::up_to_without_back_comment(expr.start(), contents).skip_trivia();

        matches!(
            tokenizer.next_back(),
            Some(SimpleToken {
                kind: SimpleTokenKind::LParen | SimpleTokenKind::LBrace | SimpleTokenKind::LBracket,
                ..
            })
        )
    } else {
        false
    }
}
