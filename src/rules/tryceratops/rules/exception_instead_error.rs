use ruff_macros::derive_message_formats;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct ErrorInsteadException;
);

impl Violation for ErrorInsteadException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use logging '.exception' instead of '.error'")
    }
}
fn visit_stmt(stmt: &Stmt) -> bool {
    if let StmtKind::Expr { value, .. } = &stmt.node {
        if let ExprKind::Call { func, .. } = &value.node {
            if let ExprKind::Attribute { attr, .. } = &func.node {
                return attr == &"error".to_string();
            }
        }
    }
    false
}

/// TRY400
pub fn error_instead_exception(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
        for stmt in body {
            if visit_stmt(stmt) {
                checker.diagnostics.push(Diagnostic::new(
                    ErrorInsteadException,
                    Range::from_located(stmt),
                ));
            }
        }
    }
}
