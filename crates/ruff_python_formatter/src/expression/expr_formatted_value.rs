use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprFormattedValue;

#[derive(Default)]
pub struct FormatExprFormattedValue;

impl FormatNodeRule<ExprFormattedValue> for FormatExprFormattedValue {
    fn fmt_fields(&self, item: &ExprFormattedValue, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
