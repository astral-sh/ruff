use ruff_python_ast::{self as ast, Expr};

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::logging;
use ruff_python_semantic::SemanticModel;

/// Collect `logging`-like calls from an AST.
pub(super) struct LoggerCandidateVisitor<'a, 'b> {
    semantic: &'a SemanticModel<'b>,
    logger_objects: &'a [String],
    pub(super) calls: Vec<&'b ast::ExprCall>,
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
            if logging::is_logger_candidate(&call.func, self.semantic, self.logger_objects) {
                self.calls.push(call);
            }
        }
        visitor::walk_expr(self, expr);
    }
}
