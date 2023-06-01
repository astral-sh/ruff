use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprAwait;

#[derive(Default)]
pub struct FormatExprAwait;

impl FormatNodeRule<ExprAwait> for FormatExprAwait {
    fn fmt_fields(&self, item: &ExprAwait, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
