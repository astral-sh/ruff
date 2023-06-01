use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtBreak;

#[derive(Default)]
pub struct FormatStmtBreak;

impl FormatNodeRule<StmtBreak> for FormatStmtBreak {
    fn fmt_fields(&self, item: &StmtBreak, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
