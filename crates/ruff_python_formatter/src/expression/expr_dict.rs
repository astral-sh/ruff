use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprDict;

#[derive(Default)]
pub struct FormatExprDict;

impl FormatNodeRule<ExprDict> for FormatExprDict {
    fn fmt_fields(&self, _item: &ExprDict, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
