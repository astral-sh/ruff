//! Specialized AST visitor trait and walk functions that only visit statements.

use crate::{self as ast, ElifElseClause, ExceptHandler, MatchCase, Stmt, Suite};

/// A trait for AST visitors that only need to visit statements.
pub trait StatementVisitor<'a> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        walk_body(self, body);
    }
    fn visit_suite(&mut self, suite: &'a Suite) {
        walk_suite(self, suite);
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

pub fn walk_suite<'a, V: StatementVisitor<'a> + ?Sized>(visitor: &mut V, suite: &'a Suite) {
    visitor.visit_body(suite);
}

pub fn walk_stmt<'a, V: StatementVisitor<'a> + ?Sized>(visitor: &mut V, stmt: &'a Stmt) {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
            visitor.visit_suite(body);
        }
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            visitor.visit_suite(body);
            visitor.visit_suite(orelse);
        }
        Stmt::ClassDef(ast::StmtClassDef { body, .. }) => {
            visitor.visit_suite(body);
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            visitor.visit_suite(body);
            visitor.visit_suite(orelse);
        }
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            visitor.visit_suite(body);
            for clause in elif_else_clauses {
                visitor.visit_elif_else_clause(clause);
            }
        }
        Stmt::With(ast::StmtWith { body, .. }) => {
            visitor.visit_suite(body);
        }
        Stmt::Match(ast::StmtMatch { cases, .. }) => {
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            visitor.visit_suite(body);
            for except_handler in handlers {
                visitor.visit_except_handler(except_handler);
            }
            visitor.visit_suite(orelse);
            visitor.visit_suite(finalbody);
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
            visitor.visit_suite(body);
        }
    }
}

pub fn walk_elif_else_clause<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    elif_else_clause: &'a ElifElseClause,
) {
    visitor.visit_suite(&elif_else_clause.body);
}

pub fn walk_match_case<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    match_case: &'a MatchCase,
) {
    visitor.visit_suite(&match_case.body);
}
