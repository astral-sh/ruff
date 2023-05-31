use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprYieldFrom;

#[derive(Default)]
pub(crate) struct FormatExprYieldFrom;

impl FormatNodeRule<ExprYieldFrom> for FormatExprYieldFrom {
    fn fmt_fields(&self, _item: &ExprYieldFrom, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
