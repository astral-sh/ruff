//! Specialized AST visitor trait and walk functions that only visit statements.

use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, MatchCase, Stmt, StmtKind};

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
    match &stmt.node {
        StmtKind::FunctionDef { body, .. } => {
            visitor.visit_body(body);
        }
        StmtKind::AsyncFunctionDef { body, .. } => {
            visitor.visit_body(body);
        }
        StmtKind::For { body, orelse, .. } => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        StmtKind::ClassDef { body, .. } => {
            visitor.visit_body(body);
        }
        StmtKind::AsyncFor { body, orelse, .. } => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        StmtKind::While { body, orelse, .. } => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        StmtKind::If { body, orelse, .. } => {
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        StmtKind::With { body, .. } => {
            visitor.visit_body(body);
        }
        StmtKind::AsyncWith { body, .. } => {
            visitor.visit_body(body);
        }
        StmtKind::Match { cases, .. } => {
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            visitor.visit_body(body);
            for excepthandler in handlers {
                visitor.visit_excepthandler(excepthandler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        StmtKind::TryStar {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
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
    match &excepthandler.node {
        ExcepthandlerKind::ExceptHandler { body, .. } => {
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
