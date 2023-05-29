use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprTuple;

#[derive(Default)]
pub struct FormatExprTuple;

impl FormatNodeRule<ExprTuple> for FormatExprTuple {
    fn fmt_fields(&self, _item: &ExprTuple, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
