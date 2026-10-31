use ruff_python_ast::StmtContinue;
use ruff_python_ast::flat;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtContinue;

impl FormatNodeRule<StmtContinue> for FormatStmtContinue {
    fn fmt_fields(
        &self,
        _item: flat::View<'_, StmtContinue>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        token("continue").fmt(f)
    }
}
