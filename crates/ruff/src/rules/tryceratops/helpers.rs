use rustpython_parser::ast::{Expr, ExprKind};

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::logging;
use ruff_python_semantic::context::Context;

/// Collect `logging`-like calls from an AST.
pub struct LoggerCandidateVisitor<'a> {
    context: &'a Context<'a>,
    pub calls: Vec<(&'a Expr, &'a Expr)>,
}

impl<'a> LoggerCandidateVisitor<'a> {
    pub fn new(context: &'a Context<'a>) -> Self {
        LoggerCandidateVisitor {
            context,
            calls: Vec::new(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for LoggerCandidateVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        if let ExprKind::Call { func, .. } = &expr.node {
            if logging::is_logger_candidate(self.context, func) {
                self.calls.push((expr, func));
            }
        }
        visitor::walk_expr(self, expr);
    }
}
