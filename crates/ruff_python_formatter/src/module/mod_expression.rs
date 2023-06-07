use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ModExpression;

#[derive(Default)]
pub struct FormatModExpression;

impl FormatNodeRule<ModExpression> for FormatModExpression {
    fn fmt_fields(&self, item: &ModExpression, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
