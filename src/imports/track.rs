use rustpython_ast::{Stmt, StmtKind};

#[derive(Debug)]
pub struct ImportTracker<'a> {
    pub blocks: Vec<Vec<&'a Stmt>>,
}

impl<'a> ImportTracker<'a> {
    pub fn new() -> Self {
        Self {
            blocks: vec![vec![]],
        }
    }

    pub fn visit_stmt(&mut self, stmt: &'a Stmt) {
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
}
