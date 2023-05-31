use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprYield;

#[derive(Default)]
pub(crate) struct FormatExprYield;

impl FormatNodeRule<ExprYield> for FormatExprYield {
    fn fmt_fields(&self, _item: &ExprYield, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
