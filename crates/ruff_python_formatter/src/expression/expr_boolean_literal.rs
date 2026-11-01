use ruff_python_ast::{ExprBooleanLiteral, SyntaxNode};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprBooleanLiteral;

impl FormatNodeRule<ruff_python_ast::kind::ExprBooleanLiteral> for FormatExprBooleanLiteral {
    fn fmt_fields(&self, item: ExprBooleanLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        if item.value() {
            token("True").fmt(f)
        } else {
            token("False").fmt(f)
        }
    }
}

impl NeedsParentheses for ExprBooleanLiteral<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
