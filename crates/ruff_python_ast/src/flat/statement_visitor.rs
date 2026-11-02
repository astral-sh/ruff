//! Specialized AST visitor trait and walk functions that only visit statements.

use crate::flat;
use crate::{ElifElseClause, ExceptHandler, MatchCase, Stmt};

/// A trait for AST visitors that only need to visit statements.
pub trait StatementVisitor<'a> {
    fn visit_body(&mut self, body: flat::Suite<'a>) {
        walk_body(self, body);
    }
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        walk_stmt(self, stmt);
    }
    fn visit_except_handler(&mut self, except_handler: ExceptHandler<'a>) {
        walk_except_handler(self, except_handler);
    }
    fn visit_elif_else_clause(&mut self, elif_else_clause: ElifElseClause<'a>) {
        walk_elif_else_clause(self, elif_else_clause);
    }
    fn visit_match_case(&mut self, match_case: MatchCase<'a>) {
        walk_match_case(self, match_case);
    }
}

pub fn walk_body<'a, V: StatementVisitor<'a> + ?Sized>(visitor: &mut V, body: flat::Suite<'a>) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_stmt<'a, V: StatementVisitor<'a> + ?Sized>(visitor: &mut V, stmt: Stmt<'a>) {
    match stmt.classify() {
        flat::StmtKind::FunctionDef(stmt_function_def) => {
            let body = stmt_function_def.body();

            visitor.visit_body(body);
        }
        flat::StmtKind::For(stmt_for) => {
            let body = stmt_for.body();
            let orelse = stmt_for.orelse();

            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        flat::StmtKind::ClassDef(stmt_class_def) => {
            let body = stmt_class_def.body();

            visitor.visit_body(body);
        }
        flat::StmtKind::While(stmt_while) => {
            let body = stmt_while.body();
            let orelse = stmt_while.orelse();

            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        flat::StmtKind::If(stmt_if) => {
            let body = stmt_if.body();
            let elif_else_clauses = stmt_if.elif_else_clauses();

            visitor.visit_body(body);
            for clause in elif_else_clauses {
                visitor.visit_elif_else_clause(clause);
            }
        }
        flat::StmtKind::With(stmt_with) => {
            let body = stmt_with.body();

            visitor.visit_body(body);
        }
        flat::StmtKind::Match(stmt_match) => {
            let cases = stmt_match.cases();

            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }
        flat::StmtKind::Try(stmt_try) => {
            let body = stmt_try.body();
            let handlers = stmt_try.handlers();
            let orelse = stmt_try.orelse();
            let finalbody = stmt_try.finalbody();

            visitor.visit_body(body);
            for except_handler in handlers {
                visitor.visit_except_handler(except_handler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        _ => {}
    }
}

pub fn walk_except_handler<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    except_handler: ExceptHandler<'a>,
) {
    match except_handler.classify() {
        flat::ExceptHandlerKind::ExceptHandler(except_handler_except_handler) => {
            let body = except_handler_except_handler.body();

            visitor.visit_body(body);
        }
    }
}

pub fn walk_elif_else_clause<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    elif_else_clause: ElifElseClause<'a>,
) {
    visitor.visit_body(elif_else_clause.body());
}

pub fn walk_match_case<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    match_case: MatchCase<'a>,
) {
    visitor.visit_body(match_case.body());
}
