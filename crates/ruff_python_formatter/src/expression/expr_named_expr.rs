use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprNamedExpr;

#[derive(Default)]
pub struct FormatExprNamedExpr;

impl FormatNodeRule<ExprNamedExpr> for FormatExprNamedExpr {
    fn fmt_fields(&self, _item: &ExprNamedExpr, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
