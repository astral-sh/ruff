use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::tryceratops::helpers::LoggerCandidateVisitor;
use crate::violation::Violation;

define_violation!(
    /// ### What it does
    /// Checks for excessive logging of the exception object
    ///
    /// ### Why is this bad?
    /// When using `logger.exception`, the exception object is logged automatically.
    ///
    /// ### Example
    /// ```python
    /// try:
    ///     ...
    /// except ValueError as e:
    ///     logger.exception(f"Found an error: {e}")
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// try:
    ///     ...
    /// except ValueError as e:
    ///     logger.exception(f"Found an error")
    /// ```
    pub struct VerboseLogMessage;
);
impl Violation for VerboseLogMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not log the exception object")
    }
}

#[derive(Default)]
pub struct NameVisitor<'a> {
    pub names: Vec<&'a Expr>,
}

impl<'a, 'b> Visitor<'b> for NameVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        if let ExprKind::Name { .. } = &expr.node {
            self.names.push(expr);
        }
        visitor::walk_expr(self, expr);
    }
}

fn check_names(checker: &mut Checker, exprs: &[&Expr], target: &str) {
    for expr in exprs {
        if let ExprKind::Name { id, .. } = &expr.node {
            if id == target {
                checker.diagnostics.push(Diagnostic::new(
                    VerboseLogMessage,
                    Range::from_located(expr),
                ));
            }
        }
    }
}

/// TRY401
pub fn verbose_log_message(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { name, body, .. } = &handler.node;
        if let Some(clean_name) = name {
            let calls = {
                let mut visitor = LoggerCandidateVisitor::default();
                visitor.visit_body(body);
                visitor.calls
            };
            for (expr, func) in calls {
                if let ExprKind::Call { args, .. } = &expr.node {
                    let mut all_names: Vec<&Expr> = vec![];
                    for arg in args {
                        let mut visitor = NameVisitor::default();
                        visitor.visit_expr(arg);
                        all_names.extend(visitor.names);
                    }
                    if let ExprKind::Attribute { attr, .. } = &func.node {
                        if attr == "exception" {
                            check_names(checker, &all_names, clean_name);
                        }
                    }
                }
            }
        }
    }
}
