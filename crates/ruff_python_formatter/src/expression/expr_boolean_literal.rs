use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprBooleanLiteral;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprBooleanLiteral;

impl FormatNodeRule<ExprBooleanLiteral> for FormatExprBooleanLiteral {
    fn fmt_fields(&self, item: &ExprBooleanLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        if item.value {
            token("True").fmt(f)
        } else {
            token("False").fmt(f)
        }
    }
}

impl NeedsParentheses for ExprBooleanLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
