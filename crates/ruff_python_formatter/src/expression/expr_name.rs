use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprName;

#[derive(Default)]
pub struct FormatExprName;

impl FormatNodeRule<ExprName> for FormatExprName {
    fn fmt_fields(&self, _item: &ExprName, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
