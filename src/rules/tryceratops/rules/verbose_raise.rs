use ruff_macros::derive_message_formats;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind, Located, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct VerboseRaise;
);
impl Violation for VerboseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use 'raise' without specifying exception name")
    }
}

/// TRY201
pub fn verbose_raise(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { name, body, .. } = &handler.node;
        // if the handler assigned a name to the exception
        if let Some(name) = name {
            // look for `raise` statements in the body
            for stmt in body {
                if let StmtKind::Raise {
                    exc: Some(box_expr),
                    ..
                } = &stmt.node
                {
                    let expr: &Located<ExprKind> = box_expr;
                    // if the the raised object is a name - check if its the same name that was
                    // assigned to the exception
                    if let ExprKind::Name { id, .. } = &expr.node {
                        if id == name {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(VerboseRaise, Range::from_located(expr)));
                        }
                    }
                }
            }
        }
    }
}
