use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtTryStar;

#[derive(Default)]
pub(crate) struct FormatStmtTryStar;

impl FormatNodeRule<StmtTryStar> for FormatStmtTryStar {
    fn fmt_fields(&self, _item: &StmtTryStar, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
