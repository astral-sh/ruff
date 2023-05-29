use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExprJoinedStr;

#[derive(Default)]
pub struct FormatExprJoinedStr;

impl FormatNodeRule<ExprJoinedStr> for FormatExprJoinedStr {
    fn fmt_fields(&self, _item: &ExprJoinedStr, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
