use ruff_python_ast::ExprBooleanLiteral;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprBooleanLiteral;

impl FormatNodeRule<ExprBooleanLiteral> for FormatExprBooleanLiteral {
    fn fmt_fields(
        &self,
        item: flat::View<'_, ExprBooleanLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        if item.value() {
            token("True").fmt(f)
        } else {
            token("False").fmt(f)
        }
    }
}

impl NeedsParentheses for flat::View<'_, ExprBooleanLiteral> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
