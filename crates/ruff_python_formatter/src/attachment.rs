use crate::cst::visitor;
use crate::cst::visitor::Visitor;
use crate::cst::{
    Alias, Arg, Body, BoolOp, CmpOp, Excepthandler, Expr, Keyword, Operator, Pattern, SliceIndex,
    Stmt, UnaryOp,
};
use crate::trivia::{decorate_trivia, TriviaIndex, TriviaToken};

struct AttachmentVisitor {
    index: TriviaIndex,
}

impl<'a> Visitor<'a> for AttachmentVisitor {
    fn visit_body(&mut self, body: &'a mut Body) {
        let trivia = self.index.body.remove(&body.id());
        if let Some(comments) = trivia {
            body.trivia.extend(comments);
        }
        visitor::walk_body(self, body);
    }

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

    fn visit_arg(&mut self, arg: &'a mut Arg) {
        let trivia = self.index.arg.remove(&arg.id());
        if let Some(comments) = trivia {
            arg.trivia.extend(comments);
        }
        visitor::walk_arg(self, arg);
    }

    fn visit_excepthandler(&mut self, excepthandler: &'a mut Excepthandler) {
        let trivia = self.index.excepthandler.remove(&excepthandler.id());
        if let Some(comments) = trivia {
            excepthandler.trivia.extend(comments);
        }
        visitor::walk_excepthandler(self, excepthandler);
    }

    fn visit_keyword(&mut self, keyword: &'a mut Keyword) {
        let trivia = self.index.keyword.remove(&keyword.id());
        if let Some(comments) = trivia {
            keyword.trivia.extend(comments);
        }
        visitor::walk_keyword(self, keyword);
    }

    fn visit_bool_op(&mut self, bool_op: &'a mut BoolOp) {
        let trivia = self.index.bool_op.remove(&bool_op.id());
        if let Some(comments) = trivia {
            bool_op.trivia.extend(comments);
        }
        visitor::walk_bool_op(self, bool_op);
    }

    fn visit_unary_op(&mut self, unary_op: &'a mut UnaryOp) {
        let trivia = self.index.unary_op.remove(&unary_op.id());
        if let Some(comments) = trivia {
            unary_op.trivia.extend(comments);
        }
        visitor::walk_unary_op(self, unary_op);
    }

    fn visit_cmp_op(&mut self, cmp_op: &'a mut CmpOp) {
        let trivia = self.index.cmp_op.remove(&cmp_op.id());
        if let Some(comments) = trivia {
            cmp_op.trivia.extend(comments);
        }
        visitor::walk_cmp_op(self, cmp_op);
    }

    fn visit_operator(&mut self, operator: &'a mut Operator) {
        let trivia = self.index.operator.remove(&operator.id());
        if let Some(comments) = trivia {
            operator.trivia.extend(comments);
        }
        visitor::walk_operator(self, operator);
    }

    fn visit_slice_index(&mut self, slice_index: &'a mut SliceIndex) {
        let trivia = self.index.slice_index.remove(&slice_index.id());
        if let Some(comments) = trivia {
            slice_index.trivia.extend(comments);
        }
        visitor::walk_slice_index(self, slice_index);
    }

    fn visit_pattern(&mut self, pattern: &'a mut Pattern) {
        let trivia = self.index.pattern.remove(&pattern.id());
        if let Some(comments) = trivia {
            pattern.trivia.extend(comments);
        }
        visitor::walk_pattern(self, pattern);
    }
}

pub fn attach(python_cst: &mut [Stmt], trivia: Vec<TriviaToken>) {
    let index = decorate_trivia(trivia, python_cst);
    let mut visitor = AttachmentVisitor { index };
    for stmt in python_cst {
        visitor.visit_stmt(stmt);
    }
}
