use ruff_python_ast::StmtBreak;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtBreak;

impl FormatNodeRule<ruff_python_ast::kind::StmtBreak> for FormatStmtBreak {
    fn fmt_fields(&self, _item: StmtBreak<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        token("break").fmt(f)
    }
}
