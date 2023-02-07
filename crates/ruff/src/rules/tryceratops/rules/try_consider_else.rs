use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct TryConsiderElse;
);
impl Violation for TryConsiderElse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider moving this statement to an `else` block")
    }
}

/// TRY300
pub fn try_consider_else(checker: &mut Checker, body: &[Stmt], orelse: &[Stmt]) {
    if body.len() > 1 && orelse.is_empty() {
        if let Some(stmt) = body.last() {
            if let StmtKind::Return { .. } = &stmt.node {
                checker
                    .diagnostics
                    .push(Diagnostic::new(TryConsiderElse, Range::from_located(stmt)));
            }
        }
    }
}
