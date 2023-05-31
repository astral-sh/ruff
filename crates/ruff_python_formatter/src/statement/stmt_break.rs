use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtBreak;

#[derive(Default)]
pub struct FormatStmtBreak;

impl FormatNodeRule<StmtBreak> for FormatStmtBreak {
    fn fmt_fields(&self, _item: &StmtBreak, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
