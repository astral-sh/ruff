use rustpython_parser::ast::{Excepthandler, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct TryConsiderElse;

impl Violation for TryConsiderElse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider moving this statement to an `else` block")
    }
}

/// TRY300
pub fn try_consider_else(
    checker: &mut Checker,
    body: &[Stmt],
    orelse: &[Stmt],
    handler: &[Excepthandler],
) {
    if body.len() > 1 && orelse.is_empty() && !handler.is_empty() {
        if let Some(stmt) = body.last() {
            if let StmtKind::Return { value } = &stmt.node {
                if let Some(value) = value {
                    if contains_effect(value, |id| checker.ctx.is_builtin(id)) {
                        return;
                    }
                }
                checker
                    .diagnostics
                    .push(Diagnostic::new(TryConsiderElse, Range::from(stmt)));
            }
        }
    }
}
