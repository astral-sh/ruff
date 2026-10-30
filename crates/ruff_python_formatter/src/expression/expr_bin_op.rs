use ruff_python_ast::ExprBinOp;
use ruff_python_ast::flat;
use ruff_python_ast::{flat::AnyNodeRef, flat::StringLike};

use crate::expression::binary_like::BinaryLike;
use crate::expression::has_parentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::string::StringLikeExtensions;

#[derive(Default)]
pub struct FormatExprBinOp;

impl FormatNodeRule<ExprBinOp> for FormatExprBinOp {
    #[inline]
    fn fmt_fields(&self, item: flat::View<'_, ExprBinOp>, f: &mut PyFormatter) -> FormatResult<()> {
        BinaryLike::Binary(item).fmt(f)
    }
}

impl NeedsParentheses for flat::View<'_, ExprBinOp> {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else if let Ok(string) = StringLike::try_from(self.left()) {
            // Multiline strings are guaranteed to never fit, avoid adding unnecessary parentheses
            if !string.is_implicit_concatenated()
                && string.is_multiline(context)
                && has_parentheses(self.right(), context).is_some()
                && !context.comments().has_dangling(*self)
                && !context.comments().has(string)
                && !context.comments().has(self.right())
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
