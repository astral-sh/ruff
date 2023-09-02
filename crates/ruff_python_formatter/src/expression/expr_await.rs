use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprAwait;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parenthesize};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprAwait;

impl FormatNodeRule<ExprAwait> for FormatExprAwait {
    fn fmt_fields(&self, item: &ExprAwait, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprAwait { range: _, value } = item;

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
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
        }
    }
}
