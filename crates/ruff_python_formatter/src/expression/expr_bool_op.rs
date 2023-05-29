use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprBoolOp;

#[derive(Default)]
pub struct FormatExprBoolOp;

impl FormatNodeRule<ExprBoolOp> for FormatExprBoolOp {
    fn fmt_fields(&self, _item: &ExprBoolOp, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
