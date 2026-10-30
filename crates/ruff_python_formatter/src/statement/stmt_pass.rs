use ruff_python_ast::StmtPass;
use ruff_python_ast::flat;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtPass;

impl FormatNodeRule<StmtPass> for FormatStmtPass {
    fn fmt_fields(&self, _item: flat::View<'_, StmtPass>, f: &mut PyFormatter) -> FormatResult<()> {
        token("pass").fmt(f)
    }
}
