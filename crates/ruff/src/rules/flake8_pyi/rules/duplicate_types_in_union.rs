use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use rustpython_parser::ast::{Expr, ExprKind, Operator};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct DuplicateTypesInUnion {
    pub duplicate_name: String,
}

impl AlwaysAutofixableViolation for DuplicateTypesInUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Duplicate {} in union", self.duplicate_name)
    }

    fn autofix_title(&self) -> String {
        format!("Remove latter {} from union", self.duplicate_name)
    }
}

///PYI016
pub fn duplicate_types_in_union(checker: &mut Checker, left: &Expr, right: &Expr) {
    // The union data structure always works like so:
    // a | b | c | d -> (((a | b) | c) | d).
    // This function gets called on each pair of brackets, so it's safe to only check if the
    // right is the duplicate (since we will have already checked the others in other invocations)

    // Collapse down the left side of the left expression into a vector of nodes
    let mut left_nodes: Vec<ExprKind> = Vec::new();
    let mut left_tree = left.node.clone();
    loop {
        if let ExprKind::BinOp {
            op: Operator::BitOr,
            left,
            right,
        } = left_tree
        {
            left_nodes.push(right.node);
            left_tree = left.node;
        } else {
            // We found a non-union node.
            // Tree traversal stops here but add this node to the vec
            left_nodes.push(left_tree);
            break;
        }
    }

    if left_nodes.contains(&right.node) {
        let mut diagnostic = Diagnostic::new(
            DuplicateTypesInUnion {
                duplicate_name: right.node.name().to_string(),
            },
            Range::from(right),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // We want to delete the "|" character as well as the duplicate
            // value, so delete from the end of "left" to the end of "right"
            diagnostic.set_fix(Edit::deletion(
                left.end_location.unwrap(),
                right.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
