use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtReturn;

#[derive(Default)]
pub struct FormatStmtReturn;

impl FormatNodeRule<StmtReturn> for FormatStmtReturn {
    fn fmt_fields(&self, item: &StmtReturn, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
