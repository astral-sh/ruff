//! Utilities for manually traversing a Python AST.
use crate::{self as ast, AnyNodeRef, ExceptHandler, Stmt};

/// Given a [`Stmt`] and its parent, return the [`ast::Suite`] that contains the [`Stmt`].
pub fn suite<'a>(stmt: &'a Stmt, parent: &'a Stmt) -> Option<EnclosingSuite<'a>> {
    // TODO: refactor this to work without a parent, ie when `stmt` is at the top level
    match parent {
        Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) => EnclosingSuite::new(body, stmt),
        Stmt::ClassDef(ast::StmtClassDef { body, .. }) => EnclosingSuite::new(body, stmt),
        Stmt::For(ast::StmtFor { body, orelse, .. }) => [body, orelse]
            .iter()
            .find_map(|suite| EnclosingSuite::new(suite, stmt)),
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => [body, orelse]
            .iter()
            .find_map(|suite| EnclosingSuite::new(suite, stmt)),
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => [body]
            .into_iter()
            .chain(elif_else_clauses.iter().map(|clause| &clause.body))
            .find_map(|suite| EnclosingSuite::new(suite, stmt)),
        Stmt::With(ast::StmtWith { body, .. }) => EnclosingSuite::new(body, stmt),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .map(|case| &case.body)
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => [body, orelse, finalbody]
            .into_iter()
            .chain(
                handlers
                    .iter()
                    .filter_map(ExceptHandler::as_except_handler)
                    .map(|handler| &handler.body),
            )
            .find_map(|suite| EnclosingSuite::new(suite, stmt)),
        _ => None,
    }
}

pub struct EnclosingSuite<'a> {
    suite: &'a [Stmt],
    position: usize,
}

impl<'a> EnclosingSuite<'a> {
    pub fn new(suite: &'a [Stmt], stmt: &'a Stmt) -> Option<Self> {
        let position = suite
            .iter()
            .position(|sibling| AnyNodeRef::ptr_eq(sibling.into(), stmt.into()))?;

        Some(EnclosingSuite { suite, position })
    }

    pub fn next_sibling(&self) -> Option<&'a Stmt> {
        self.suite.get(self.position + 1)
    }

    pub fn next_siblings(&self) -> &'a [Stmt] {
        self.suite.get(self.position + 1..).unwrap_or_default()
    }

    pub fn previous_sibling(&self) -> Option<&'a Stmt> {
        self.suite.get(self.position.checked_sub(1)?)
    }
}

impl std::ops::Deref for EnclosingSuite<'_> {
    type Target = [Stmt];

    fn deref(&self) -> &Self::Target {
        self.suite
    }
}
