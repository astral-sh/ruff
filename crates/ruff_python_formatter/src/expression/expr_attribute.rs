use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprAttribute;

#[derive(Default)]
pub(crate) struct FormatExprAttribute;

impl FormatNodeRule<ExprAttribute> for FormatExprAttribute {
    fn fmt_fields(&self, _item: &ExprAttribute, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
