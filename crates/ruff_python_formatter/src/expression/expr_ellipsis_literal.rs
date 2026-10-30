use ruff_python_ast::ExprEllipsisLiteral;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprEllipsisLiteral;

impl FormatNodeRule<ExprEllipsisLiteral> for FormatExprEllipsisLiteral {
    fn fmt_fields(
        &self,
        _item: flat::View<'_, ExprEllipsisLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        token("...").fmt(f)
    }
}

impl NeedsParentheses for flat::View<'_, ExprEllipsisLiteral> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
