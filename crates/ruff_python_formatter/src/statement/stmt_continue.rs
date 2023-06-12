use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtContinue;

#[derive(Default)]
pub struct FormatStmtContinue;

impl FormatNodeRule<StmtContinue> for FormatStmtContinue {
    fn fmt_fields(&self, item: &StmtContinue, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
