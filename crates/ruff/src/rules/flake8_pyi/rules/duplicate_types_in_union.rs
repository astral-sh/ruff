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
pub fn duplicate_types_in_union(checker: &mut Checker, left_expr: &Expr, potential_dup: &Expr) {
    // The union data structure always works like so:
    // a | b | c | d -> (((a | b) | c) | d).
    // This function gets called on each pair of brackets, so it's safe to only check if the
    // right is the duplicate (since we will have already checked the others in other invocations)

    let mut working_tree = left_expr.node.clone();

    if loop {
        if let ExprKind::BinOp {
            op: Operator::BitOr,
            left: left_node,
            right: right_node,
        } = working_tree
        {
            if right_node.node == potential_dup.node {
                // Early exit - we only care if there _is_ a duplicate, not how many
                break true;
            }
            working_tree = left_node.node;
            continue;
        }
        // We found a non-union node.
        // Tree traversal stops here, but still check this node
        break working_tree == potential_dup.node;
    } {
        // If we broke the loop with true, create a violation
        let mut diagnostic = Diagnostic::new(
            DuplicateTypesInUnion {
                duplicate_name: potential_dup.node.name().to_string(),
            },
            Range::from(potential_dup),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // We want to delete the "|" character as well as the duplicate
            // value, so delete from the end of "left" to the end of "right"
            diagnostic.set_fix(Edit::deletion(
                left_expr.end_location.unwrap(),
                potential_dup.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
