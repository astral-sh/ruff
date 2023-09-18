use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprFormattedValue;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprFormattedValue;

impl FormatNodeRule<ExprFormattedValue> for FormatExprFormattedValue {
    fn fmt_fields(&self, _item: &ExprFormattedValue, _f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprFString");
    }
}

impl NeedsParentheses for ExprFormattedValue {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
