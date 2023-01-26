use ruff_macros::derive_message_formats;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind};

use crate::ast::helpers::is_logger_candidate;
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
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

#[derive(Default)]
/// Collect `logging.error`-like calls from an AST. Matches `logging.error`,
/// `logger.error`, `self.logger.error`, etc., but not arbitrary `foo.error`
/// calls.
struct ErrorCallVisitor<'a> {
    calls: Vec<&'a Expr>,
}

impl<'a, 'b> Visitor<'b> for ErrorCallVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        if let ExprKind::Call { func, .. } = &expr.node {
            if is_logger_candidate(func, Some("error")) {
                self.calls.push(expr);
            }
        }
        visitor::walk_expr(self, expr);
    }
}

/// TRY400
pub fn error_instead_of_exception(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
        let calls = {
            let mut visitor = ErrorCallVisitor::default();
            visitor.visit_body(body);
            visitor.calls
        };
        for expr in calls {
            checker.diagnostics.push(Diagnostic::new(
                ErrorInsteadOfException,
                Range::from_located(expr),
            ));
        }
    }
}
