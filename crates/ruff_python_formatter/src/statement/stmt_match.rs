use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtMatch;

#[derive(Default)]
pub struct FormatStmtMatch;

impl FormatNodeRule<StmtMatch> for FormatStmtMatch {
    fn fmt_fields(&self, item: &StmtMatch, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
