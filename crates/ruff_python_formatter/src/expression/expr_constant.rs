use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprConstant;

#[derive(Default)]
pub(crate) struct FormatExprConstant;

impl FormatNodeRule<ExprConstant> for FormatExprConstant {
    fn fmt_fields(&self, _item: &ExprConstant, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
