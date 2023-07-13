use crate::statement::stmt_try::AnyStatementTry;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::Format;
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtTryStar;

#[derive(Default)]
pub struct FormatStmtTryStar;

impl FormatNodeRule<StmtTryStar> for FormatStmtTryStar {
    fn fmt_fields(&self, item: &StmtTryStar, f: &mut PyFormatter) -> FormatResult<()> {
        AnyStatementTry::from(item).fmt(f)
    }

    fn fmt_dangling_comments(&self, _node: &StmtTryStar, _f: &mut PyFormatter) -> FormatResult<()> {
        // dangling comments are formatted as part of AnyStatementTry::fmt
        Ok(())
    }
}
