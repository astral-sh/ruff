use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprIfExp;

#[derive(Default)]
pub struct FormatExprIfExp;

impl FormatNodeRule<ExprIfExp> for FormatExprIfExp {
    fn fmt_fields(&self, _item: &ExprIfExp, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
