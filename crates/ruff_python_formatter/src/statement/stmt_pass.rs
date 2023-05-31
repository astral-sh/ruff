use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtPass;

#[derive(Default)]
pub struct FormatStmtPass;

impl FormatNodeRule<StmtPass> for FormatStmtPass {
    fn fmt_fields(&self, _item: &StmtPass, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
