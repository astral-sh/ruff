use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtIf;

#[derive(Default)]
pub struct FormatStmtIf;

impl FormatNodeRule<StmtIf> for FormatStmtIf {
    fn fmt_fields(&self, item: &StmtIf, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
