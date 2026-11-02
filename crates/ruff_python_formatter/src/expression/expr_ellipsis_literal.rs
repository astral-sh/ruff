use ruff_python_ast::{ExprEllipsisLiteral, SyntaxNode};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprEllipsisLiteral;

impl FormatNodeRule<ruff_python_ast::kind::ExprEllipsisLiteral> for FormatExprEllipsisLiteral {
    fn fmt_fields(&self, _item: ExprEllipsisLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        token("...").fmt(f)
    }
}

impl NeedsParentheses for ExprEllipsisLiteral<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
