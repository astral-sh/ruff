//! Specialized AST visitor trait and walk functions that only visit statements.

use crate::{self as ast, ElifElseClause, ExceptHandler, MatchCase, Stmt};

/// A trait for AST visitors that only need to visit statements.
pub trait StatementVisitor<'a, 'ast> {
    fn visit_body(&mut self, body: &'a [Stmt<'ast>]) {
        walk_body(self, body);
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt<'ast>) {
        walk_stmt(self, stmt);
    }
    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler<'ast>) {
        walk_except_handler(self, except_handler);
    }
    fn visit_elif_else_clause(&mut self, elif_else_clause: &'a ElifElseClause<'ast>) {
        walk_elif_else_clause(self, elif_else_clause);
    }
    fn visit_match_case(&mut self, match_case: &'a MatchCase<'ast>) {
        walk_match_case(self, match_case);
    }
}

pub fn walk_body<'a, 'ast, V: StatementVisitor<'a, 'ast> + ?Sized>(
    visitor: &mut V,
    body: &'a [Stmt<'ast>],
) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_stmt<'a, 'ast, V: StatementVisitor<'a, 'ast> + ?Sized>(
    visitor: &mut V,
    stmt: &'a Stmt<'ast>,
) {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
            visitor.visit_body(body);
        }
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::ClassDef(ast::StmtClassDef { body, .. }) => {
            visitor.visit_body(body);
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            visitor.visit_body(body);
            for clause in elif_else_clauses {
                visitor.visit_elif_else_clause(clause);
            }
        }
        Stmt::With(ast::StmtWith { body, .. }) => {
            visitor.visit_body(body);
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

pub fn walk_except_handler<'a, 'ast, V: StatementVisitor<'a, 'ast> + ?Sized>(
    visitor: &mut V,
    except_handler: &'a ExceptHandler<'ast>,
) {
    match except_handler {
        ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) => {
            visitor.visit_body(body);
        }
    }
}

pub fn walk_elif_else_clause<'a, 'ast, V: StatementVisitor<'a, 'ast> + ?Sized>(
    visitor: &mut V,
    elif_else_clause: &'a ElifElseClause<'ast>,
) {
    visitor.visit_body(&elif_else_clause.body);
}

pub fn walk_match_case<'a, 'ast, V: StatementVisitor<'a, 'ast> + ?Sized>(
    visitor: &mut V,
    match_case: &'a MatchCase<'ast>,
) {
    visitor.visit_body(&match_case.body);
}
