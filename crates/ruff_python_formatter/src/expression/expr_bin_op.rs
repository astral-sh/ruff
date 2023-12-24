use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprBinOp;

use crate::comments::SourceComment;
use crate::expression::binary_like::BinaryLike;
use crate::expression::expr_string_literal::is_multiline_string;
use crate::expression::has_parentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprBinOp;

impl FormatNodeRule<ExprBinOp> for FormatExprBinOp {
    #[inline]
    fn fmt_fields(&self, item: &ExprBinOp, f: &mut PyFormatter) -> FormatResult<()> {
        BinaryLike::Binary(item).fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled inside of `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprBinOp {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else if let Some(literal_expr) = self.left.as_literal_expr() {
            // Multiline strings are guaranteed to never fit, avoid adding unnecessary parentheses
            if !literal_expr.is_implicit_concatenated()
                && is_multiline_string(literal_expr.into(), context.source())
                && has_parentheses(&self.right, context).is_some()
                && !context.comments().has_dangling(self)
                && !context.comments().has(literal_expr)
                && !context.comments().has(self.right.as_ref())
            {
                OptionalParentheses::Never
            } else {
                OptionalParentheses::Multiline
            }
        } else {
            OptionalParentheses::Multiline
        }
    }
}
