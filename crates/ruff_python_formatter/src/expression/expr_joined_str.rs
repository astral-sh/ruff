use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprJoinedStr;

#[derive(Default)]
pub struct FormatExprJoinedStr;

impl FormatNodeRule<ExprJoinedStr> for FormatExprJoinedStr {
    fn fmt_fields(&self, item: &ExprJoinedStr, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
