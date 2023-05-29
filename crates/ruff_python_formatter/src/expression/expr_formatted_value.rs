use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprFormattedValue;

#[derive(Default)]
pub struct FormatExprFormattedValue;

impl FormatNodeRule<ExprFormattedValue> for FormatExprFormattedValue {
    fn fmt_fields(&self, _item: &ExprFormattedValue, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
