use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprIfExp;

#[derive(Default)]
pub struct FormatExprIfExp;

impl FormatNodeRule<ExprIfExp> for FormatExprIfExp {
    fn fmt_fields(&self, item: &ExprIfExp, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
