//! Specialized AST visitor trait and walk functions that only visit statements.

use crate::{self as ast, ElifElseClause, ExceptHandler, MatchCase, Stmt};

/// A trait for AST visitors that only need to visit statements.
pub trait StatementVisitor<'a> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        walk_body(self, body);
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }
    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        walk_except_handler(self, except_handler);
    }
    fn visit_elif_else_clause(&mut self, elif_else_clause: &'a ElifElseClause) {
        walk_elif_else_clause(self, elif_else_clause);
    }
    fn visit_match_case(&mut self, match_case: &'a MatchCase) {
        walk_match_case(self, match_case);
    }
}

pub fn walk_body<'a, V: StatementVisitor<'a> + ?Sized>(visitor: &mut V, body: &'a [Stmt]) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_stmt<'a, V: StatementVisitor<'a> + ?Sized>(visitor: &mut V, stmt: &'a Stmt) {
    match stmt {
        Stmt::FunctionDef(node) => {
            visitor.visit_body(&node.body);
        }
        Stmt::For(node) => {
            visitor.visit_body(&node.body);
            visitor.visit_body(&node.orelse);
        }
        Stmt::ClassDef(node) => {
            visitor.visit_body(&node.body);
        }
        Stmt::While(node) => {
            visitor.visit_body(&node.body);
            visitor.visit_body(&node.orelse);
        }
        Stmt::If(node) => {
            visitor.visit_body(&node.body);
            for clause in &node.elif_else_clauses {
                visitor.visit_elif_else_clause(clause);
            }
        }
        Stmt::With(node) => {
            visitor.visit_body(&node.body);
        }
        Stmt::Match(node) => {
            for match_case in &node.cases {
                visitor.visit_match_case(match_case);
            }
        }
        Stmt::Try(node) => {
            visitor.visit_body(&node.body);
            for except_handler in &node.handlers {
                visitor.visit_except_handler(except_handler);
            }
            visitor.visit_body(&node.orelse);
            visitor.visit_body(&node.finalbody);
        }
        _ => {}
    }
}

pub fn walk_except_handler<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    except_handler: &'a ExceptHandler,
) {
    match except_handler {
        ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) => {
            visitor.visit_body(body);
        }
    }
}

pub fn walk_elif_else_clause<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    elif_else_clause: &'a ElifElseClause,
) {
    visitor.visit_body(&elif_else_clause.body);
}

pub fn walk_match_case<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    match_case: &'a MatchCase,
) {
    visitor.visit_body(&match_case.body);
}
