use rustpython_parser::ast::Stmt;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct Assert;
);
impl Violation for Assert {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `assert` detected")
    }
}

/// S101
pub fn assert_used(stmt: &Stmt) -> Diagnostic {
    Diagnostic::new(
        Assert,
        Range::new(stmt.location, stmt.location.with_col_offset("assert".len())),
    )
}
