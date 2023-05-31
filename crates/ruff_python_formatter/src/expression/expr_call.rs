use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprCall;

#[derive(Default)]
pub(crate) struct FormatExprCall;

impl FormatNodeRule<ExprCall> for FormatExprCall {
    fn fmt_fields(&self, _item: &ExprCall, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
