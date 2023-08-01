use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtNonlocal;

use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtNonlocal;

impl FormatNodeRule<StmtNonlocal> for FormatStmtNonlocal {
    fn fmt_fields(&self, item: &StmtNonlocal, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [text("nonlocal"), space()])?;

        f.join_with(format_args![text(","), space()])
            .entries(item.names.iter().formatted())
            .finish()
    }
}
