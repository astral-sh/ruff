use ruff_python_ast::StmtIpyEscapeCommand;
use ruff_python_ast::flat;
use ruff_text_size::Ranged;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtIpyEscapeCommand;

impl FormatNodeRule<StmtIpyEscapeCommand> for FormatStmtIpyEscapeCommand {
    fn fmt_fields(
        &self,
        item: flat::View<'_, StmtIpyEscapeCommand>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        source_text_slice(item.range()).fmt(f)
    }
}
