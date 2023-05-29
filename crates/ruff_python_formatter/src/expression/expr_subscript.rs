use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprSubscript;

#[derive(Default)]
pub struct FormatExprSubscript;

impl FormatNodeRule<ExprSubscript> for FormatExprSubscript {
    fn fmt_fields(&self, _item: &ExprSubscript, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
