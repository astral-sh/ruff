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
    offsets: Vec<Vec<usize>>,
}

impl<'a> SourceCodeLocator<'a> {
    pub fn new(content: &'a str) -> Self {
        SourceCodeLocator {
            content,
            offsets: Self::compute_offsets(content),
        }
    }

    fn compute_offsets(content: &str) -> Vec<Vec<usize>> {
        let mut offsets = vec![];
        let mut offset = 0;
        for line in content.lines() {
            let mut newline = 0;
            let mut line_offsets: Vec<usize> = vec![];
            for (i, char) in line.char_indices() {
                line_offsets.push(offset + i);
                newline = i + char.len_utf8();
            }
            line_offsets.push(offset + newline);
            offsets.push(line_offsets);
            offset += newline + 1;
        }
        offsets.push(vec![offset]);
        offsets
    }

    pub fn slice_source_code_at(&self, location: &Location) -> &'a str {
        let offset = self.offsets[location.row() - 1][location.column() - 1];
        &self.content[offset..]
    }

    pub fn slice_source_code_range(&self, range: &Range) -> &'a str {
        let start = self.offsets[range.location.row() - 1][range.location.column() - 1];
        let end = self.offsets[range.end_location.row() - 1][range.end_location.column() - 1];
        &self.content[start..end]
    }

    pub fn partition_source_code_at(
        &self,
        outer: &Range,
        inner: &Range,
    ) -> (&'a str, &'a str, &'a str) {
        let outer_start = self.offsets[outer.location.row() - 1][outer.location.column() - 1];
        let outer_end = self.offsets[outer.end_location.row() - 1][outer.end_location.column() - 1];
        let inner_start = self.offsets[inner.location.row() - 1][inner.location.column() - 1];
        let inner_end = self.offsets[inner.end_location.row() - 1][inner.end_location.column() - 1];
        (
            &self.content[outer_start..inner_start],
            &self.content[inner_start..inner_end],
            &self.content[inner_end..outer_end],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SourceCodeLocator;

    #[test]
    fn source_code_locator_init() {
        let content = "# \u{4e9c}\nclass Foo:\n    \"\"\".\"\"\"";
        let locator = SourceCodeLocator::new(content);
        assert_eq!(locator.offsets.len(), 4);
        assert_eq!(locator.offsets[0], [0, 1, 2, 5]);
        assert_eq!(locator.offsets[1], [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(
            locator.offsets[2],
            [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28]
        );
        assert_eq!(locator.offsets[3], [29]);
    }
}
