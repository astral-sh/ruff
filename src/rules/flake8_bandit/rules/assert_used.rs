use rustpython_ast::{Located, StmtKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// S101
pub fn assert_used(stmt: &Located<StmtKind>) -> Diagnostic {
    Diagnostic::new(
        violations::AssertUsed,
        Range::new(stmt.location, stmt.location.with_col_offset("assert".len())),
    )
}
