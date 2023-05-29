use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprSlice;

#[derive(Default)]
pub struct FormatExprSlice;

impl FormatNodeRule<ExprSlice> for FormatExprSlice {
    fn fmt_fields(&self, _item: &ExprSlice, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
