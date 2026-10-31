use ruff_python_ast::ExprNoneLiteral;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprNoneLiteral;

impl FormatNodeRule<ExprNoneLiteral> for FormatExprNoneLiteral {
    fn fmt_fields(
        &self,
        _item: flat::View<'_, ExprNoneLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        token("None").fmt(f)
    }
}

impl NeedsParentheses for flat::View<'_, ExprNoneLiteral> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
