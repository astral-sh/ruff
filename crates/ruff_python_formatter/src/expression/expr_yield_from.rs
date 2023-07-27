use rustpython_ast::ExprYieldFrom;
use ruff_formatter::{Format, FormatResult};
use crate::{FormatNodeRule, PyFormatter};
use crate::expression::expr_yield::AnyExpressionYield;

#[derive(Default)]
pub struct FormatExprYieldFrom;

impl FormatNodeRule<ExprYieldFrom> for FormatExprYieldFrom {
    fn fmt_fields(&self, item: &ExprYieldFrom, f: &mut PyFormatter) -> FormatResult<()> {
        AnyExpressionYield::from(item).fmt(f)
    }
}
