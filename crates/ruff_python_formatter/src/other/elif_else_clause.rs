use ruff_python_ast::ElifElseClause;

use crate::prelude::*;
use crate::statement::stmt_if::format_elif_else_clause;
use crate::statement::suite::SuiteKind;

/// Note that this implementation misses the leading newlines before the leading comments because
/// it does not have access to the last node of the previous branch. The `StmtIf` therefore doesn't
/// call this but `format_elif_else_clause` directly.
#[derive(Default)]
pub struct FormatElifElseClause;

impl FormatNodeRule<ElifElseClause> for FormatElifElseClause {
    fn fmt_fields(&self, item: &ElifElseClause, f: &mut PyFormatter) -> FormatResult<()> {
        format_elif_else_clause(
            item,
            f,
            None,
            SuiteKind::Other {
                // For stability, we can't insert an empty line if we don't know if the outer suite
                // also does.
                last_suite_in_statement: true,
            },
        )
    }
}
