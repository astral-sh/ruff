use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::tryceratops::helpers::LoggerCandidateVisitor;
use crate::violation::Violation;

define_violation!(
    pub struct VerboseLogMessage;
);
impl Violation for VerboseLogMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not log the exception object")
    }
}

/// TRY401
pub fn verbose_log_message(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
        let calls = {
            let mut visitor = LoggerCandidateVisitor::default();
            visitor.visit_body(body);
            visitor.calls
        };
        for (expr, func) in calls {
            if let ExprKind::Attribute { attr, .. } = &func.node {
                if attr == "exception" {
                    checker.diagnostics.push(Diagnostic::new(
                        VerboseLogMessage,
                        Range::from_located(expr),
                    ));
                }
            }
        }
    }
}
