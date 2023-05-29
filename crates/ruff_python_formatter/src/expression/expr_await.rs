use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprAwait;

#[derive(Default)]
pub struct FormatExprAwait;

impl FormatNodeRule<ExprAwait> for FormatExprAwait {
    fn fmt_fields(&self, _item: &ExprAwait, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
