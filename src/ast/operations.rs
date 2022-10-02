use rustpython_parser::ast::{Constant, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::{BindingKind, Range, Scope};

/// Extract the names bound to a given __all__ assignment.
pub fn extract_all_names(stmt: &Stmt, scope: &Scope) -> Vec<String> {
    let mut names: Vec<String> = vec![];

    fn add_to_names(names: &mut Vec<String>, elts: &[Expr]) {
        for elt in elts {
            if let ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } = &elt.node
            {
                names.push(value.to_string())
            }
        }
    }

    // Grab the existing bound __all__ values.
    if let StmtKind::AugAssign { .. } = &stmt.node {
        if let Some(binding) = scope.values.get("__all__") {
            if let BindingKind::Export(existing) = &binding.kind {
                names.extend_from_slice(existing);
            }
        }
    }

    if let Some(value) = match &stmt.node {
        StmtKind::Assign { value, .. } => Some(value),
        StmtKind::AnnAssign { value, .. } => value.as_ref(),
        StmtKind::AugAssign { value, .. } => Some(value),
        _ => None,
    } {
        match &value.node {
            ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
                add_to_names(&mut names, elts)
            }
            ExprKind::BinOp { left, right, .. } => {
                let mut current_left = left;
                let mut current_right = right;
                while let Some(elts) = match &current_right.node {
                    ExprKind::List { elts, .. } => Some(elts),
                    ExprKind::Tuple { elts, .. } => Some(elts),
                    _ => None,
                } {
                    add_to_names(&mut names, elts);
                    match &current_left.node {
                        ExprKind::BinOp { left, right, .. } => {
                            current_left = left;
                            current_right = right;
                        }
                        ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
                            add_to_names(&mut names, elts);
                            break;
                        }
                        _ => break,
                    }
                }
            }
            _ => {}
        }
    }

    names
}

/// Check if a node is parent of a conditional branch.
pub fn on_conditional_branch(parent_stack: &[usize], parents: &[&Stmt]) -> bool {
    for index in parent_stack.iter().rev() {
        let parent = parents[*index];
        if matches!(parent.node, StmtKind::If { .. } | StmtKind::While { .. }) {
            return true;
        }
        if let StmtKind::Expr { value } = &parent.node {
            if matches!(value.node, ExprKind::IfExp { .. }) {
                return true;
            }
        }
    }

    false
}

/// Check if a node is in a nested block.
pub fn in_nested_block(parent_stack: &[usize], parents: &[&Stmt]) -> bool {
    for index in parent_stack.iter().rev() {
        let parent = parents[*index];
        if matches!(
            parent.node,
            StmtKind::Try { .. } | StmtKind::If { .. } | StmtKind::With { .. }
        ) {
            return true;
        }
    }

    false
}

/// Check if a node represents an unpacking assignment.
pub fn is_unpacking_assignment(stmt: &Stmt) -> bool {
    if let StmtKind::Assign { targets, value, .. } = &stmt.node {
        if !targets.iter().any(|child| {
            matches!(
                child.node,
                ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. }
            )
        }) {
            return false;
        }
        match &value.node {
            ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. } => return false,
            _ => {}
        }
        return true;
    }
    false
}

/// Struct used to efficiently slice source code at (row, column) Locations.
pub struct SourceCodeLocator<'a> {
    content: &'a str,
    offsets: Vec<usize>,
    initialized: bool,
}

impl<'a> SourceCodeLocator<'a> {
    pub fn new(content: &'a str) -> Self {
        SourceCodeLocator {
            content,
            offsets: vec![],
            initialized: false,
        }
    }

    pub fn slice_source_code_at(&mut self, location: &Location) -> &'a str {
        if !self.initialized {
            let mut offset = 0;
            for i in self.content.lines() {
                self.offsets.push(offset);
                offset += i.len();
                offset += 1;
            }
            self.initialized = true;
        }
        let offset = self.offsets[location.row() - 1] + location.column() - 1;
        &self.content[offset..]
    }

    pub fn slice_source_code_range(&mut self, range: &Range) -> &'a str {
        if !self.initialized {
            let mut offset = 0;
            for i in self.content.lines() {
                self.offsets.push(offset);
                offset += i.len();
                offset += 1;
            }
            self.initialized = true;
        }
        let start = self.offsets[range.location.row() - 1] + range.location.column() - 1;
        let end = self.offsets[range.end_location.row() - 1] + range.end_location.column() - 1;
        &self.content[start..end]
    }
}
