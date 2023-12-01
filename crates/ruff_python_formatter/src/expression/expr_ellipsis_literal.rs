use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprEllipsisLiteral;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprEllipsisLiteral;

impl FormatNodeRule<ExprEllipsisLiteral> for FormatExprEllipsisLiteral {
    fn fmt_fields(&self, _item: &ExprEllipsisLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        token("...").fmt(f)
    }
}

impl NeedsParentheses for ExprEllipsisLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
