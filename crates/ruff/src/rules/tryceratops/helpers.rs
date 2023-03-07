use rustpython_parser::ast::{Expr, ExprKind};

use ruff_python_ast::helpers::is_logger_candidate;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

#[derive(Default)]
/// Collect `logging`-like calls from an AST.
pub struct LoggerCandidateVisitor<'a> {
    pub calls: Vec<(&'a Expr, &'a Expr)>,
}

impl<'a, 'b> Visitor<'b> for LoggerCandidateVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        if let ExprKind::Call { func, .. } = &expr.node {
            if is_logger_candidate(func) {
                self.calls.push((expr, func));
            }
        }
        visitor::walk_expr(self, expr);
    }
}
