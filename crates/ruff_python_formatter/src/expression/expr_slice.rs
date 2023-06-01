use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprSlice;

#[derive(Default)]
pub struct FormatExprSlice;

impl FormatNodeRule<ExprSlice> for FormatExprSlice {
    fn fmt_fields(&self, item: &ExprSlice, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
