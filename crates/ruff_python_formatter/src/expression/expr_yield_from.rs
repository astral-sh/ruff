use ruff_python_ast::{ExprYieldFrom, SyntaxNode};

use crate::expression::expr_yield::AnyExpressionYield;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprYieldFrom;

impl FormatNodeRule<ruff_python_ast::kind::ExprYieldFrom> for FormatExprYieldFrom {
    fn fmt_fields(&self, item: ExprYieldFrom<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        AnyExpressionYield::from(item).fmt(f)
    }
}

impl NeedsParentheses for ExprYieldFrom<'_> {
    fn needs_parentheses(
        &self,
        parent: SyntaxNode,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        AnyExpressionYield::from(*self).needs_parentheses(parent, context)
    }
}
