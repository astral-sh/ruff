use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAnnAssign;

#[derive(Default)]
pub struct FormatStmtAnnAssign;

impl FormatNodeRule<StmtAnnAssign> for FormatStmtAnnAssign {
    fn fmt_fields(&self, _item: &StmtAnnAssign, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
