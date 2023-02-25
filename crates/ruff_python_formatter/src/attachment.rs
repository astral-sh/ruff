use crate::core::visitor;
use crate::core::visitor::Visitor;
use crate::cst::{Alias, Excepthandler, Expr, SliceIndex, Stmt};
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

    fn visit_alias(&mut self, alias: &'a mut Alias) {
        let trivia = self.index.alias.remove(&alias.id());
        if let Some(comments) = trivia {
            alias.trivia.extend(comments);
        }
        visitor::walk_alias(self, alias);
    }

    fn visit_excepthandler(&mut self, excepthandler: &'a mut Excepthandler) {
        let trivia = self.index.excepthandler.remove(&excepthandler.id());
        if let Some(comments) = trivia {
            excepthandler.trivia.extend(comments);
        }
        visitor::walk_excepthandler(self, excepthandler);
    }

    fn visit_slice_index(&mut self, slice_index: &'a mut SliceIndex) {
        let trivia = self.index.slice_index.remove(&slice_index.id());
        if let Some(comments) = trivia {
            slice_index.trivia.extend(comments);
        }
        visitor::walk_slice_index(self, slice_index);
    }
}

pub fn attach(python_cst: &mut [Stmt], trivia: Vec<TriviaToken>) {
    let index = decorate_trivia(trivia, python_cst);
    AttachmentVisitor { index }.visit_body(python_cst);
}
