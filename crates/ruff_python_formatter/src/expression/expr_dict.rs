use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprDict;

#[derive(Default)]
pub struct FormatExprDict;

impl FormatNodeRule<ExprDict> for FormatExprDict {
    fn fmt_fields(&self, item: &ExprDict, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
