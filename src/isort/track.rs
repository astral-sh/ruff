use rustpython_ast::{Excepthandler, Stmt, StmtKind};

use crate::ast::visitor::Visitor;

#[derive(Debug)]
pub struct ImportTracker<'a> {
    blocks: Vec<Vec<&'a Stmt>>,
}
impl<'a> ImportTracker<'a> {
    pub fn new() -> Self {
        Self {
            blocks: vec![vec![]],
        }
    }

    pub fn next(&mut self) -> Option<Vec<&'a Stmt>> {
        self.blocks.pop()
    }
}

impl<'a, 'b> Visitor<'b> for ImportTracker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        let index = self.blocks.len() - 1;
        if matches!(
            stmt.node,
            StmtKind::Import { .. } | StmtKind::ImportFrom { .. }
        ) {
            self.blocks[index].push(stmt);
        } else {
            if !self.blocks[index].is_empty() {
                self.blocks.push(vec![]);
            }
        }
    }

    fn visit_excepthandler(&mut self, _: &'a Excepthandler) {
        let index = self.blocks.len() - 1;
        if !self.blocks[index].is_empty() {
            self.blocks.push(vec![]);
        }
    }
}
