use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprYieldFrom;

use crate::expression::expr_yield::AnyExpressionYield;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprYieldFrom;

impl FormatNodeRule<ExprYieldFrom> for FormatExprYieldFrom {
    fn fmt_fields(&self, item: &ExprYieldFrom, f: &mut PyFormatter) -> FormatResult<()> {
        AnyExpressionYield::from(item).fmt(f)
    }
}

impl NeedsParentheses for ExprYieldFrom {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        AnyExpressionYield::from(self).needs_parentheses(parent, context)
    }
}
