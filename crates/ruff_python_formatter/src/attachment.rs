use crate::core::visitor;
use crate::core::visitor::Visitor;
use crate::cst::{Expr, Stmt};
use crate::trivia::{decorate_trivia, TriviaIndex, TriviaToken};

struct AttachmentVisitor {
    index: TriviaIndex,
}

impl<'a> Visitor<'a> for AttachmentVisitor {
    fn visit_stmt(&mut self, stmt: &'a mut Stmt) {
        let trivia = self.index.stmt.remove(&stmt.id());
        if let Some(comments) = trivia {
            stmt.trivia.extend(comments);
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a mut Expr) {
        let trivia = self.index.expr.remove(&expr.id());
        if let Some(comments) = trivia {
            expr.trivia.extend(comments);
        }
        visitor::walk_expr(self, expr);
    }
}

pub fn attach(python_cst: &mut [Stmt], trivia: Vec<TriviaToken>) {
    let index = decorate_trivia(trivia, python_cst);
    AttachmentVisitor { index }.visit_body(python_cst);
}
