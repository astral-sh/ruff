use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprBinOp;

#[derive(Default)]
pub struct FormatExprBinOp;

impl FormatNodeRule<ExprBinOp> for FormatExprBinOp {
    fn fmt_fields(&self, _item: &ExprBinOp, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
