use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprDictComp;

#[derive(Default)]
pub(crate) struct FormatExprDictComp;

impl FormatNodeRule<ExprDictComp> for FormatExprDictComp {
    fn fmt_fields(&self, _item: &ExprDictComp, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
