use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Located, StmtKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct AssertUsed;
);
impl Violation for AssertUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `assert` detected")
    }
}

/// S101
pub fn assert_used(stmt: &Located<StmtKind>) -> Diagnostic {
    Diagnostic::new(
        AssertUsed,
        Range::new(stmt.location, stmt.location.with_col_offset("assert".len())),
    )
}
