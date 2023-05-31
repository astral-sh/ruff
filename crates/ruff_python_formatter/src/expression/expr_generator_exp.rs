use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprGeneratorExp;

#[derive(Default)]
pub struct FormatExprGeneratorExp;

impl FormatNodeRule<ExprGeneratorExp> for FormatExprGeneratorExp {
    fn fmt_fields(&self, _item: &ExprGeneratorExp, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
