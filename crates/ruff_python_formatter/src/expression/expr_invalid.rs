use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprInvalid;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprInvalid;

impl FormatNodeRule<ExprInvalid> for FormatExprInvalid {
    fn fmt_fields(&self, item: &ExprInvalid, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [source_text_slice(item.range)])
    }
}

impl NeedsParentheses for ExprInvalid {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
