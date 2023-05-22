use rustpython_parser::ast::{self, Expr};

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::logging;
use ruff_python_semantic::model::SemanticModel;

/// Collect `logging`-like calls from an AST.
pub(crate) struct LoggerCandidateVisitor<'a, 'b> {
    context: &'a SemanticModel<'b>,
    pub(crate) calls: Vec<(&'b Expr, &'b Expr)>,
}

impl<'a, 'b> LoggerCandidateVisitor<'a, 'b> {
    pub(crate) fn new(context: &'a SemanticModel<'b>) -> Self {
        LoggerCandidateVisitor {
            context,
            calls: Vec::new(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for LoggerCandidateVisitor<'a, 'b> {
    fn visit_expr(&mut self, expr: &'b Expr) {
        if let Expr::Call(ast::ExprCall { func, .. }) = expr {
            if logging::is_logger_candidate(func, self.context) {
                self.calls.push((expr, func));
            }
        }
        visitor::walk_expr(self, expr);
    }
}
