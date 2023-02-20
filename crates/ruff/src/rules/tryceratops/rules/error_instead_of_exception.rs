use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::tryceratops::helpers::LoggerCandidateVisitor;
use crate::violation::Violation;

define_violation!(
    pub struct ErrorInsteadOfException;
);
impl Violation for ErrorInsteadOfException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `logging.exception` instead of `logging.error`")
    }
}

/// TRY400
pub fn error_instead_of_exception(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
        let calls = {
            let mut visitor = LoggerCandidateVisitor::default();
            visitor.visit_body(body);
            visitor.calls
        };
        for (expr, func) in calls {
            if let ExprKind::Attribute { attr, .. } = &func.node {
                if attr == "error" {
                    checker.diagnostics.push(Diagnostic::new(
                        ErrorInsteadOfException,
                        Range::from_located(expr),
                    ));
                }
            }
        }
    }
}
