use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprBinOp;

use crate::expression::binary_like::BinaryLike;
use crate::expression::has_parentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::string::AnyString;

#[derive(Default)]
pub struct FormatExprBinOp;

impl FormatNodeRule<ExprBinOp> for FormatExprBinOp {
    #[inline]
    fn fmt_fields(&self, item: &ExprBinOp, f: &mut PyFormatter) -> FormatResult<()> {
        BinaryLike::Binary(item).fmt(f)
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
        } else if let Some(string) = AnyString::from_expression(&self.left) {
            // Multiline strings are guaranteed to never fit, avoid adding unnecessary parentheses
            if !string.is_implicit_concatenated()
                && string.is_multiline(context.source())
                && has_parentheses(&self.right, context).is_some()
                && !context.comments().has_dangling(self)
                && !context.comments().has(string)
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
