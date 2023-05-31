use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprSet;

#[derive(Default)]
pub(crate) struct FormatExprSet;

impl FormatNodeRule<ExprSet> for FormatExprSet {
    fn fmt_fields(&self, _item: &ExprSet, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
