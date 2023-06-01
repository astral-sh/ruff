use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprYield;

#[derive(Default)]
pub struct FormatExprYield;

impl FormatNodeRule<ExprYield> for FormatExprYield {
    fn fmt_fields(&self, item: &ExprYield, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
