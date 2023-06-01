use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtExpr;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: &StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
