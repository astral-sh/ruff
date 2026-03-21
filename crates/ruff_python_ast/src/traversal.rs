//! Utilities for manually traversing a Python AST.
use crate::{self as ast, AnyNodeRef, ExceptHandler, Stmt};

/// Given a [`Stmt`] and its parent, return the [`ast::Suite`] that contains the [`Stmt`].
pub fn suite<'a>(
    stmt: impl Into<AnyNodeRef<'a>>,
    parent: impl Into<AnyNodeRef<'a>>,
) -> Option<EnclosingSuite<'a>> {
    // TODO: refactor this to work without a parent, ie when `stmt` is at the top level
    let stmt = stmt.into();
    match parent.into() {
        AnyNodeRef::ModModule(ast::ModModule { body, .. }) => EnclosingSuite::new(body, stmt),
        AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. }) => {
            EnclosingSuite::new(body, stmt)
        }
        AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. }) => EnclosingSuite::new(body, stmt),
        AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. }) => [body, orelse]
            .iter()
            .find_map(|suite| EnclosingSuite::new(suite, stmt)),
        AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => [body, orelse]
            .iter()
            .find_map(|suite| EnclosingSuite::new(suite, stmt)),
        AnyNodeRef::StmtIf(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => [body]
            .into_iter()
            .chain(elif_else_clauses.iter().map(|clause| &clause.body))
            .find_map(|suite| EnclosingSuite::new(suite, stmt)),
        AnyNodeRef::StmtWith(ast::StmtWith { body, .. }) => EnclosingSuite::new(body, stmt),
        AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .map(|case| &case.body)
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        AnyNodeRef::StmtTry(ast::StmtTry {
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
    pub fn new(suite: &'a [Stmt], stmt: AnyNodeRef<'a>) -> Option<Self> {
        let position = suite
            .iter()
            .position(|sibling| AnyNodeRef::ptr_eq(sibling.into(), stmt))?;

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
