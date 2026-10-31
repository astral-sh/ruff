use ruff_python_ast::{
    ExceptHandler, Expr, ExprName,
    flat::{self, fields as ast},
};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::visitor;
use ruff_python_ast::flat::visitor::Visitor;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
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
///
/// ## Options
///
/// - `lint.logger-objects`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.250", category = Category::Complexity)]
pub(crate) struct VerboseLogMessage;

impl Violation for VerboseLogMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Redundant exception object included in `logging.exception` call".to_string()
    }
}

/// TRY401
pub(crate) fn verbose_log_message(checker: &Checker, handlers: flat::NodeList<'_, ExceptHandler>) {
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) =
            handler.fields();

        // Find all calls to `logging.exception`.
        let calls = {
            let mut visitor =
                LoggerCandidateVisitor::new(checker.semantic(), &checker.settings().logger_objects);
            visitor.visit_body(body);
            visitor.calls
        };

        for (expr, logging_level) in calls {
            if matches!(logging_level, LoggingLevel::Exception) {
                // Collect all referenced names in the `logging.exception` call.
                let names: Vec<flat::View<'_, ExprName>> = {
                    expr.arguments()
                        .args()
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
                        checker.report_diagnostic(VerboseLogMessage, expr.range());
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct NameVisitor<'a> {
    names: Vec<flat::View<'a, ExprName>>,
}

impl<'a> Visitor<'a> for NameVisitor<'a> {
    fn visit_expr(&mut self, expr: flat::View<'a, Expr>) {
        match expr.classify() {
            flat::ExprKind::Name(name) if name.ctx().is_load() => self.names.push(name),
            flat::ExprKind::Attribute(_) => {}
            _ => visitor::walk_expr(self, expr),
        }
    }
}
