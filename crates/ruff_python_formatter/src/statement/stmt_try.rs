use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtTry;

#[derive(Default)]
pub struct FormatStmtTry;

impl FormatNodeRule<StmtTry> for FormatStmtTry {
    fn fmt_fields(&self, item: &StmtTry, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }

    fn fmt_dangling_comments(&self, _node: &StmtTry, _f: &mut PyFormatter) -> FormatResult<()> {
        // TODO(konstin): Needs node formatting or this leads to unstable formatting
        Ok(())
    }
}
