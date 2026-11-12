use super::{AnyNode, Suite};
use crate::Stmt;

pub fn suite<'a>(
    stmt: impl Into<crate::SyntaxNode<'a>>,
    parent: impl Into<crate::SyntaxNode<'a>>,
) -> Option<EnclosingSuite<'a>> {
    // TODO: refactor this to work without a parent, ie when `stmt` is at the top level
    let stmt = stmt.into();
    match parent.into().classify() {
        AnyNode::ModModule(node) => EnclosingSuite::new(node.body(), stmt),
        AnyNode::StmtFunctionDef(node) => EnclosingSuite::new(node.body(), stmt),
        AnyNode::StmtClassDef(node) => EnclosingSuite::new(node.body(), stmt),
        AnyNode::StmtWith(node) => EnclosingSuite::new(node.body(), stmt),
        AnyNode::StmtIf(node) => std::iter::once(node.body())
            .chain(
                node.elif_else_clauses()
                    .iter()
                    .map(crate::ElifElseClause::body),
            )
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        AnyNode::StmtMatch(node) => node
            .cases()
            .iter()
            .find_map(|case| EnclosingSuite::new(case.body(), stmt)),
        AnyNode::ElifElseClause(node) => EnclosingSuite::new(node.body(), stmt),
        AnyNode::MatchCase(node) => EnclosingSuite::new(node.body(), stmt),
        AnyNode::ExceptHandlerExceptHandler(node) => EnclosingSuite::new(node.body(), stmt),
        AnyNode::StmtFor(node) => [node.body(), node.orelse()]
            .into_iter()
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        AnyNode::StmtWhile(node) => [node.body(), node.orelse()]
            .into_iter()
            .find_map(|body| EnclosingSuite::new(body, stmt)),
        AnyNode::StmtTry(node) => [node.body(), node.orelse(), node.finalbody()]
            .into_iter()
            .chain(
                node.handlers()
                    .iter()
                    .filter_map(super::super::generated::ExceptHandler::as_except_handler)
                    .map(crate::ExceptHandlerExceptHandler::body),
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
    pub fn new(suite: Suite<'a>, stmt: crate::SyntaxNode<'a>) -> Option<Self> {
        let position = suite.position_of(stmt)?;

        Some(EnclosingSuite { suite, position })
    }

    pub fn next_sibling(&self) -> Option<Stmt<'a>> {
        self.suite.get(self.position + 1)
    }

    pub fn next_siblings(&self) -> Suite<'a> {
        self.suite.slice(self.position + 1..self.suite.len())
    }

    pub fn previous_sibling(&self) -> Option<Stmt<'a>> {
        self.suite.get(self.position.checked_sub(1)?)
    }
}
