use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprDictComp;

#[derive(Default)]
pub struct FormatExprDictComp;

impl FormatNodeRule<ExprDictComp> for FormatExprDictComp {
    fn fmt_fields(&self, item: &ExprDictComp, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
