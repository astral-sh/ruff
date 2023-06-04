use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtTryStar;

#[derive(Default)]
pub struct FormatStmtTryStar;

impl FormatNodeRule<StmtTryStar> for FormatStmtTryStar {
    fn fmt_fields(&self, item: &StmtTryStar, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
