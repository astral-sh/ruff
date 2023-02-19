use crate::ast::helpers::is_logger_candidate;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use rustpython_parser::ast::{Expr, ExprKind};

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
