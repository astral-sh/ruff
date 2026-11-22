use ruff_python_ast::StmtPass;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtPass;

impl FormatNodeRule<ruff_python_ast::kind::StmtPass> for FormatStmtPass {
    fn fmt_fields(&self, _item: StmtPass<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        token("pass").fmt(f)
    }
}
