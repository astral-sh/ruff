use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprSetComp;

#[derive(Default)]
pub struct FormatExprSetComp;

impl FormatNodeRule<ExprSetComp> for FormatExprSetComp {
    fn fmt_fields(&self, _item: &ExprSetComp, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
