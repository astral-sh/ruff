//! Specialized AST visitor trait and walk functions that only visit statements.

use rustpython_parser::ast::{self, Excepthandler, MatchCase, Stmt};

/// A trait for AST visitors that only need to visit statements.
pub trait StatementVisitor<'a> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        walk_body(self, body);
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }
    fn visit_excepthandler(&mut self, excepthandler: &'a Excepthandler) {
        walk_excepthandler(self, excepthandler);
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
    match &stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => {
            visitor.visit_body(body);
        }
        Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. }) => {
            visitor.visit_body(body);
        }
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::ClassDef(ast::StmtClassDef { body, .. }) => {
            visitor.visit_body(body);
        }
        Stmt::AsyncFor(ast::StmtAsyncFor { body, orelse, .. }) => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::If(ast::StmtIf { body, orelse, .. }) => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::With(ast::StmtWith { body, .. }) => {
            visitor.visit_body(body);
        }
        Stmt::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
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
            range: _range,
        }) => {
            visitor.visit_body(body);
            for excepthandler in handlers {
                visitor.visit_excepthandler(excepthandler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        Stmt::TryStar(ast::StmtTryStar {
            body,
            handlers,
            orelse,
            finalbody,
            range: _range,
        }) => {
            visitor.visit_body(body);
            for excepthandler in handlers {
                visitor.visit_excepthandler(excepthandler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        _ => {}
    }
}

pub fn walk_excepthandler<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    excepthandler: &'a Excepthandler,
) {
    match &excepthandler {
        Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { body, .. }) => {
            visitor.visit_body(body);
        }
    }
}

pub fn walk_match_case<'a, V: StatementVisitor<'a> + ?Sized>(
    visitor: &mut V,
    match_case: &'a MatchCase,
) {
    visitor.visit_body(&match_case.body);
}
