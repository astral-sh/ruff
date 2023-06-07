use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAugAssign;

#[derive(Default)]
pub struct FormatStmtAugAssign;

impl FormatNodeRule<StmtAugAssign> for FormatStmtAugAssign {
    fn fmt_fields(&self, item: &StmtAugAssign, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
