use ruff_python_ast::{self as ast, ExceptHandler, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

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
/// except ValueError:
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

/// TRY401
pub(crate) fn verbose_log_message(checker: &mut Checker, handlers: &[ExceptHandler]) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) = handler;

        // Find all calls to `logging.exception`.
        let calls = {
            let mut visitor =
                LoggerCandidateVisitor::new(checker.semantic(), &checker.settings.logger_objects);
            visitor.visit_body(body);
            visitor.calls
        };

        for (expr, logging_level) in calls {
            if matches!(logging_level, LoggingLevel::Exception) {
                // Collect all referenced names in the `logging.exception` call.
                let names: Vec<&ast::ExprName> = {
                    expr.arguments
                        .args
                        .iter()
                        .flat_map(|arg| {
                            let mut visitor = NameVisitor::default();
                            visitor.visit_expr(arg);
                            visitor.names
                        })
                        .collect()
                };

                // Find any bound exceptions in the call.
                for expr in names {
                    let Some(id) = checker.semantic().resolve_name(expr) else {
                        continue;
                    };
                    let binding = checker.semantic().binding(id);
                    if binding.kind.is_bound_exception() {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(VerboseLogMessage, expr.range()));
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct NameVisitor<'a> {
    names: Vec<&'a ast::ExprName>,
}

impl<'a> Visitor<'a> for NameVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) if name.ctx.is_load() => self.names.push(name),
            Expr::Attribute(_) => {}
            _ => visitor::walk_expr(self, expr),
        }
    }
}
