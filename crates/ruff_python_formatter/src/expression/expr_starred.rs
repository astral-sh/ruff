use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprStarred;

#[derive(Default)]
pub struct FormatExprStarred;

impl FormatNodeRule<ExprStarred> for FormatExprStarred {
    fn fmt_fields(&self, _item: &ExprStarred, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
