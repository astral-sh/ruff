use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprAttribute;

#[derive(Default)]
pub struct FormatExprAttribute;

impl FormatNodeRule<ExprAttribute> for FormatExprAttribute {
    fn fmt_fields(&self, item: &ExprAttribute, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
