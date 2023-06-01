use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprStarred;

#[derive(Default)]
pub struct FormatExprStarred;

impl FormatNodeRule<ExprStarred> for FormatExprStarred {
    fn fmt_fields(&self, item: &ExprStarred, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
