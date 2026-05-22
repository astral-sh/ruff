use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprAwait;
use ruff_text_size::Ranged;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parenthesize, is_expression_parenthesized,
    is_type_annotation_of,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprAwait;

impl FormatNodeRule<ExprAwait> for FormatExprAwait {
    fn fmt_fields(&self, item: &ExprAwait, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprAwait {
            range: _,
            node_index: _,
            value,
        } = item;

        write!(
            f,
            [
                token("await"),
                space(),
                maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
            ]
        )
    }
}

impl NeedsParentheses for ExprAwait {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() || is_type_annotation_of(self.range(), parent) {
            OptionalParentheses::Always
        } else if is_expression_parenthesized(
            self.value.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        ) {
            // Prefer splitting the value if it is parenthesized.
            OptionalParentheses::Never
        } else {
            self.value.needs_parentheses(self.into(), context)
        }
    }
}
