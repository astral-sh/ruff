use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, ExceptHandler, Expr};
use ruff_python_semantic::analyze::logging;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::logging::LoggingLevel;

/// Collect `logging`-like calls from an AST.
pub(super) struct LoggerCandidateVisitor<'a, 'b> {
    semantic: &'a SemanticModel<'b>,
    logger_objects: &'a [String],
    pub(super) calls: Vec<(&'b ast::ExprCall, LoggingLevel)>,
}

impl<'a, 'b> LoggerCandidateVisitor<'a, 'b> {
    pub(super) fn new(semantic: &'a SemanticModel<'b>, logger_objects: &'a [String]) -> Self {
        LoggerCandidateVisitor {
            semantic,
            logger_objects,
            calls: Vec::new(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for LoggerCandidateVisitor<'a, 'b> {
    fn visit_expr(&mut self, expr: &'b Expr) {
        if let Expr::Call(call) = expr {
            match call.func.as_ref() {
                Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                    if logging::is_logger_candidate(&call.func, self.semantic, self.logger_objects)
                    {
                        if let Some(logging_level) = LoggingLevel::from_attribute(attr) {
                            self.calls.push((call, logging_level));
                        };
                    }
                }
                Expr::Name(_) => {
                    if let Some(qualified_name) =
                        self.semantic.resolve_qualified_name(call.func.as_ref())
                    {
                        if let ["logging", attribute] = qualified_name.segments() {
                            if let Some(logging_level) = LoggingLevel::from_attribute(attribute) {
                                {
                                    self.calls.push((call, logging_level));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        visitor::walk_expr(self, expr);
    }

    fn visit_except_handler(&mut self, _except_handler: &'b ExceptHandler) {
        // Don't recurse into exception handlers, since we'll re-run the visitor on any such
        // handlers.
    }
}
