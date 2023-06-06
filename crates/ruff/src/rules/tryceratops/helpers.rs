use rustpython_parser::ast::{self, Expr};

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::logging;
use ruff_python_semantic::model::SemanticModel;

/// Collect `logging`-like calls from an AST.
pub(crate) struct LoggerCandidateVisitor<'a, 'b> {
    semantic_model: &'a SemanticModel<'b>,
    pub(crate) calls: Vec<&'b ast::ExprCall>,
}

impl<'a, 'b> LoggerCandidateVisitor<'a, 'b> {
    pub(crate) fn new(semantic_model: &'a SemanticModel<'b>) -> Self {
        LoggerCandidateVisitor {
            semantic_model,
            calls: Vec::new(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for LoggerCandidateVisitor<'a, 'b> {
    fn visit_expr(&mut self, expr: &'b Expr) {
        if let Expr::Call(call) = expr {
            if logging::is_logger_candidate(&call.func, self.semantic_model) {
                self.calls.push(call);
            }
        }
        visitor::walk_expr(self, expr);
    }
}
