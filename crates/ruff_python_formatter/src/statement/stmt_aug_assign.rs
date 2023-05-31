use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAugAssign;

#[derive(Default)]
pub(crate) struct FormatStmtAugAssign;

impl FormatNodeRule<StmtAugAssign> for FormatStmtAugAssign {
    fn fmt_fields(&self, _item: &StmtAugAssign, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
