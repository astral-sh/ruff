use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ModExpression;

#[derive(Default)]
pub struct FormatModExpression;

impl FormatNodeRule<ModExpression> for FormatModExpression {
    fn fmt_fields(&self, _item: &ModExpression, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
