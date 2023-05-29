use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprYieldFrom;

#[derive(Default)]
pub struct FormatExprYieldFrom;

impl FormatNodeRule<ExprYieldFrom> for FormatExprYieldFrom {
    fn fmt_fields(&self, _item: &ExprYieldFrom, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
