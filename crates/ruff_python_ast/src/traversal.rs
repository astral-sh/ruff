//! Utilities for manually traversing a Python AST.
use crate::{self as ast, ExceptHandler, Stmt, Suite};

/// Given a [`Stmt`] and its parent, return the [`Suite`] that contains the [`Stmt`].
pub fn suite<'a>(stmt: &'a Stmt, parent: &'a Stmt) -> Option<&'a Suite> {
    // TODO: refactor this to work without a parent, ie when `stmt` is at the top level
    match parent {
        Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => Some(body),
        Stmt::ClassDef(ast::StmtClassDef { body, .. }) => Some(body),
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            if body.contains(stmt) {
                Some(body)
            } else if orelse.contains(stmt) {
                Some(orelse)
            } else {
                None
            }
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            if body.contains(stmt) {
                Some(body)
            } else if orelse.contains(stmt) {
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
            if body.contains(stmt) {
                Some(body)
            } else {
                elif_else_clauses
                    .iter()
                    .map(|elif_else_clause| &elif_else_clause.body)
                    .find(|body| body.contains(stmt))
            }
        }
        Stmt::With(ast::StmtWith { body, .. }) => Some(body),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .map(|case| &case.body)
            .find(|body| body.contains(stmt)),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            if body.contains(stmt) {
                Some(body)
            } else if orelse.contains(stmt) {
                Some(orelse)
            } else if finalbody.contains(stmt) {
                Some(finalbody)
            } else {
                handlers
                    .iter()
                    .filter_map(ExceptHandler::as_except_handler)
                    .map(|handler| &handler.body)
                    .find(|body| body.contains(stmt))
            }
        }
        _ => None,
    }
}

/// Given a [`Stmt`] and its containing [`Suite`], return the next [`Stmt`] in the [`Suite`].
pub fn next_sibling<'a>(stmt: &'a Stmt, suite: &'a Suite) -> Option<&'a Stmt> {
    let mut iter = suite.iter();
    while let Some(sibling) = iter.next() {
        if sibling == stmt {
            return iter.next();
        }
    }
    None
}
