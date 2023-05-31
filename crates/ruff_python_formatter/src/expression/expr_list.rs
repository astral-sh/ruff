use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprList;

#[derive(Default)]
pub(crate) struct FormatExprList;

impl FormatNodeRule<ExprList> for FormatExprList {
    fn fmt_fields(&self, _item: &ExprList, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
