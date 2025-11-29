use ruff_python_ast::StmtContinue;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtContinue;

impl FormatNodeRule<StmtContinue> for FormatStmtContinue {
    fn fmt_fields(&self, _item: &StmtContinue, f: &mut PyFormatter) -> FormatResult<()> {
        token("continue").fmt(f)
    }
    fn is_suppressed(&self, node: &StmtContinue, context: &PyFormatContext) -> bool {
        context.is_suppressed(node.into())
    }
}
