use ruff_python_ast::StmtBreak;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtBreak;

impl FormatNodeRule<StmtBreak> for FormatStmtBreak {
    fn fmt_fields(&self, _item: &StmtBreak, f: &mut PyFormatter) -> FormatResult<()> {
        token("break").fmt(f)
    }
    fn is_suppressed(&self, node: &StmtBreak, context: &PyFormatContext) -> bool {
        context.is_suppressed(node.into())
    }
}
