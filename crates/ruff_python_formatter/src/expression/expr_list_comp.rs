use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprListComp;

#[derive(Default)]
pub struct FormatExprListComp;

impl FormatNodeRule<ExprListComp> for FormatExprListComp {
    fn fmt_fields(&self, _item: &ExprListComp, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
