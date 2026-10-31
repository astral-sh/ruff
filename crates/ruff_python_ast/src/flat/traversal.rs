use super::{Any, AnyKind, Suite, View};
use crate::Stmt;

pub fn suite<'a>(
    stmt: impl Into<View<'a, Any>>,
    parent: impl Into<View<'a, Any>>,
) -> Option<EnclosingSuite<'a>> {
    // TODO: refactor this to work without a parent, ie when `stmt` is at the top level
    let stmt = stmt.into();
    match parent.into().classify() {
        AnyKind::ModModule(node) => EnclosingSuite::new(node.body(), stmt),
        AnyKind::StmtFunctionDef(node) => EnclosingSuite::new(node.body(), stmt),
        AnyKind::StmtClassDef(node) => EnclosingSuite::new(node.body(), stmt),
        AnyKind::StmtWith(node) => EnclosingSuite::new(node.body(), stmt),
        AnyKind::StmtIf(node) => std::iter::once(node.body())
            .chain(
                node.elif_else_clauses()
                    .iter()
                    .map(View::<crate::ElifElseClause>::body),
            )
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        AnyKind::StmtMatch(node) => node
            .cases()
            .iter()
            .find_map(|case| EnclosingSuite::new(case.body(), stmt)),
        AnyKind::ElifElseClause(node) => EnclosingSuite::new(node.body(), stmt),
        AnyKind::MatchCase(node) => EnclosingSuite::new(node.body(), stmt),
        AnyKind::ExceptHandlerExceptHandler(node) => EnclosingSuite::new(node.body(), stmt),
        AnyKind::StmtFor(node) => [node.body(), node.orelse()]
            .into_iter()
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        AnyKind::StmtWhile(node) => [node.body(), node.orelse()]
            .into_iter()
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        AnyKind::StmtTry(node) => [node.body(), node.orelse(), node.finalbody()]
            .into_iter()
            .chain(
                node.handlers()
                    .iter()
                    .filter_map(View::as_except_handler)
                    .map(View::<crate::ExceptHandlerExceptHandler>::body),
            )
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        _ => None,
    }
}

pub struct EnclosingSuite<'a> {
    suite: Suite<'a>,
    position: usize,
}

impl<'a> EnclosingSuite<'a> {
    pub fn new(suite: Suite<'a>, stmt: View<'a, Any>) -> Option<Self> {
        let position = suite
            .iter()
            .position(|sibling| sibling.is_same_node(stmt))?;

        Some(EnclosingSuite { suite, position })
    }

    pub fn next_sibling(&self) -> Option<View<'a, Stmt>> {
        self.suite.get(self.position + 1)
    }

    pub fn next_siblings(&self) -> Suite<'a> {
        self.suite.slice(self.position + 1..self.suite.len())
    }

    pub fn previous_sibling(&self) -> Option<View<'a, Stmt>> {
        self.suite.get(self.position.checked_sub(1)?)
    }
}
