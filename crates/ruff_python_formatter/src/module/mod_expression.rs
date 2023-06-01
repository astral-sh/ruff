use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ModExpression;

#[derive(Default)]
pub struct FormatModExpression;

impl FormatNodeRule<ModExpression> for FormatModExpression {
    fn fmt_fields(&self, item: &ModExpression, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
