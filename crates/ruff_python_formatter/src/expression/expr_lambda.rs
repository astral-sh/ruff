use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprLambda;

#[derive(Default)]
pub struct FormatExprLambda;

impl FormatNodeRule<ExprLambda> for FormatExprLambda {
    fn fmt_fields(&self, _item: &ExprLambda, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
