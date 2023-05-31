use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprUnaryOp;

#[derive(Default)]
pub struct FormatExprUnaryOp;

impl FormatNodeRule<ExprUnaryOp> for FormatExprUnaryOp {
    fn fmt_fields(&self, _item: &ExprUnaryOp, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
