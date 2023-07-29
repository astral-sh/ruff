use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtGlobal;

use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtGlobal;

impl FormatNodeRule<StmtGlobal> for FormatStmtGlobal {
    fn fmt_fields(&self, item: &StmtGlobal, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [text("global"), space()])?;

        f.join_with(format_args![text(","), space()])
            .entries(item.names.iter().formatted())
            .finish()
    }
}
