use crate::define_simple_violation;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Located, StmtKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;

define_simple_violation!(AssertUsed, "Use of `assert` detected");

/// S101
pub fn assert_used(stmt: &Located<StmtKind>) -> Diagnostic {
    Diagnostic::new(
        AssertUsed,
        Range::new(stmt.location, stmt.location.with_col_offset("assert".len())),
    )
}
