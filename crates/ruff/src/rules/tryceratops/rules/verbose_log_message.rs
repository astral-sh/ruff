use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;
use crate::rules::tryceratops::helpers::LoggerCandidateVisitor;

/// ## What it does
/// Checks for excessive logging of exception objects.
///
/// ## Why is this bad?
/// When logging exceptions via `logging.exception`, the exception object
/// is logged automatically. Including the exception object in the log
/// message is redundant and can lead to excessive logging.
///
/// ## Example
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
///     logger.exception("Found an error")
/// ```
#[violation]
pub struct VerboseLogMessage;

impl Violation for VerboseLogMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Redundant exception object included in `logging.exception` call")
    }
}

#[derive(Default)]
struct NameVisitor<'a> {
    names: Vec<(&'a str, &'a Expr)>,
}

impl<'a, 'b> Visitor<'b> for NameVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Name {
                id,
                ctx: ExprContext::Load,
            } => self.names.push((id, expr)),
            ExprKind::Attribute { .. } => {}
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// TRY401
pub fn verbose_log_message(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { name, body, .. } = &handler.node;
        let Some(target) = name else {
            continue;
        };

        // Find all calls to `logging.exception`.
        let calls = {
            let mut visitor = LoggerCandidateVisitor::new(&checker.ctx);
            visitor.visit_body(body);
            visitor.calls
        };

        for (expr, func) in calls {
            let ExprKind::Call { args, .. } = &expr.node else {
                continue;
            };
            if let ExprKind::Attribute { attr, .. } = &func.node {
                if attr == "exception" {
                    // Collect all referenced names in the `logging.exception` call.
                    let names: Vec<(&str, &Expr)> = {
                        let mut names = Vec::new();
                        for arg in args {
                            let mut visitor = NameVisitor::default();
                            visitor.visit_expr(arg);
                            names.extend(visitor.names);
                        }
                        names
                    };
                    for (id, expr) in names {
                        if id == target {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(VerboseLogMessage, Range::from(expr)));
                        }
                    }
                }
            }
        }
    }
}
