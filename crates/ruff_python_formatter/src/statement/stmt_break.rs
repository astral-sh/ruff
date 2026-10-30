use ruff_python_ast::StmtBreak;
use ruff_python_ast::flat;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtBreak;

impl FormatNodeRule<StmtBreak> for FormatStmtBreak {
    fn fmt_fields(
        &self,
        _item: flat::View<'_, StmtBreak>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        token("break").fmt(f)
    }
}
