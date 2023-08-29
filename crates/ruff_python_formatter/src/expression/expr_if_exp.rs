use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Expr, ExprIfExp};

use crate::comments::leading_comments;
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break_or_space,
    is_expression_parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprIfExp {
    layout: ExprIfExpLayout,
}

#[derive(Default, Copy, Clone)]
pub enum ExprIfExpLayout {
    #[default]
    Default,
    Nested,
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
            // We place `if test` and `else orelse` on a single line, so the `test` and `orelse` leading
            // comments go on the line before the `if` or `else` instead of directly ahead `test` or
            // `orelse`
            write!(
                f,
                [
                    body.format(),
                    in_parentheses_only_soft_line_break_or_space(),
                    leading_comments(comments.leading(test.as_ref())),
                    text("if"),
                    space(),
                    test.format(),
                    in_parentheses_only_soft_line_break_or_space(),
                    leading_comments(comments.leading(orelse.as_ref())),
                    text("else"),
                    space(),
                ]
            )?;

            FormatOrElse { orelse }.fmt(f)
        });

        if matches!(self.layout, ExprIfExpLayout::Nested) {
            // Nested if expressions should not be given a new group
            inner.fmt(f)
        } else {
            in_parentheses_only_group(&inner).fmt(f)
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

struct FormatOrElse<'a> {
    orelse: &'a Expr,
}

impl Format<PyFormatContext<'_>> for FormatOrElse<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self.orelse {
            Expr::IfExp(if_expr)
                if !is_expression_parenthesized(if_expr.into(), f.context().source()) =>
            {
                // Mark nested if expressions e.g. `a if a else b if b else c` and avoid creating a new group
                write!(f, [if_expr.format().with_options(ExprIfExpLayout::Nested)])
            }
            _ => write!(f, [in_parentheses_only_group(&self.orelse.format())]),
        }
    }
}
