use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprNamedExpr;

#[derive(Default)]
pub struct FormatExprNamedExpr;

impl FormatNodeRule<ExprNamedExpr> for FormatExprNamedExpr {
    fn fmt_fields(&self, item: &ExprNamedExpr, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
