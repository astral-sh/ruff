use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtDelete;

#[derive(Default)]
pub struct FormatStmtDelete;

impl FormatNodeRule<StmtDelete> for FormatStmtDelete {
    fn fmt_fields(&self, item: &StmtDelete, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
