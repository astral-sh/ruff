use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct Assert;

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
