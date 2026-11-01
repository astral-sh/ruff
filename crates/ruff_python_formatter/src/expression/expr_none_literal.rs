use ruff_python_ast::{ExprNoneLiteral, SyntaxNode};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprNoneLiteral;

impl FormatNodeRule<ruff_python_ast::kind::ExprNoneLiteral> for FormatExprNoneLiteral {
    fn fmt_fields(&self, _item: ExprNoneLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        token("None").fmt(f)
    }
}

impl NeedsParentheses for ExprNoneLiteral<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
