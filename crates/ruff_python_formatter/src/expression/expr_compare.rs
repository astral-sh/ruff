use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprCompare;

#[derive(Default)]
pub(crate) struct FormatExprCompare;

impl FormatNodeRule<ExprCompare> for FormatExprCompare {
    fn fmt_fields(&self, _item: &ExprCompare, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
