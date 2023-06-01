use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtNonlocal;

#[derive(Default)]
pub struct FormatStmtNonlocal;

impl FormatNodeRule<StmtNonlocal> for FormatStmtNonlocal {
    fn fmt_fields(&self, item: &StmtNonlocal, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
