use ruff_python_ast::StmtContinue;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtContinue;

impl FormatNodeRule<ruff_python_ast::kind::StmtContinue> for FormatStmtContinue {
    fn fmt_fields(&self, _item: StmtContinue<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        token("continue").fmt(f)
    }
}
