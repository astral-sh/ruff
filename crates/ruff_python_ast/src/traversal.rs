//! Utilities for manually traversing a Python AST.
use crate::{self as ast, AnyNodeRef, ExceptHandler, Stmt, Suite};

/// Given a [`Stmt`] and its parent, return the [`Suite`] that contains the [`Stmt`].
pub fn suite<'a>(stmt: &'a Stmt, parent: &'a Stmt) -> Option<&'a Suite> {
    fn contains_same(suite: &[Stmt], stmt: &Stmt) -> bool {
        suite
            .iter()
            .any(|sibling| AnyNodeRef::ptr_eq(sibling.into(), stmt.into()))
    }

    // TODO: refactor this to work without a parent, ie when `stmt` is at the top level
    match parent {
        Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => Some(body),
        Stmt::ClassDef(ast::StmtClassDef { body, .. }) => Some(body),
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            if contains_same(body, stmt) {
                Some(body)
            } else if contains_same(orelse, stmt) {
                Some(orelse)
            } else {
                None
            }
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            if contains_same(body, stmt) {
                Some(body)
            } else if contains_same(orelse, stmt) {
                Some(orelse)
            } else {
                None
            }
        }
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            if contains_same(body, stmt) {
                Some(body)
            } else {
                elif_else_clauses
                    .iter()
                    .map(|elif_else_clause| &elif_else_clause.body)
                    .find(|body| contains_same(body, stmt))
            }
        }
        Stmt::With(ast::StmtWith { body, .. }) => Some(body),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .map(|case| &case.body)
            .find(|body| contains_same(body, stmt)),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            if contains_same(body, stmt) {
                Some(body)
            } else if contains_same(orelse, stmt) {
                Some(orelse)
            } else if contains_same(finalbody, stmt) {
                Some(finalbody)
            } else {
                handlers
                    .iter()
                    .filter_map(ExceptHandler::as_except_handler)
                    .map(|handler| &handler.body)
                    .find(|body| contains_same(body, stmt))
            }
        }
        _ => None,
    }
}

/// Given a [`Stmt`] and its containing [`Suite`], return the next [`Stmt`] in the [`Suite`].
pub fn next_sibling<'a>(stmt: &'a Stmt, suite: &'a Suite) -> Option<&'a Stmt> {
    let mut iter = suite.iter();
    while let Some(sibling) = iter.next() {
        if AnyNodeRef::ptr_eq(sibling.into(), stmt.into()) {
            return iter.next();
        }
    }
    None
}
