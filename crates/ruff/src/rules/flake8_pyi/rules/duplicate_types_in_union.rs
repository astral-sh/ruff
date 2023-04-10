use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::types::Range;
use rustc_hash::FxHashSet;
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
pub fn duplicate_types_in_union<'a>(
    mut seen_nodes: FxHashSet<ComparableExpr<'a>>,
    checker: &mut Checker,
    expr: &'a Expr,
    previous: Option<&'a Expr>,
) -> FxHashSet<ComparableExpr<'a>> {
    // The union data structure usually works like so:
    // a | b | c -> (a | b) | c
    // But can be forced via brackets to any structure:
    // a | (b | c)
    // So we need to check the tree fully - left first, then right so we always emit on the latter
    // duplicate(s)

    if let ExprKind::BinOp {
        op: Operator::BitOr,
        left,
        right,
    } = &expr.node
    {
        // traverse left, then right, assigning the previous node as needed
        seen_nodes = duplicate_types_in_union(seen_nodes, checker, left, previous);
        seen_nodes = duplicate_types_in_union(seen_nodes, checker, right, Some(left));
        return seen_nodes;
    }
    // If was already in the set, raise a violation
    if !seen_nodes.insert(expr.into()) {
        let mut diagnostic = Diagnostic::new(
            DuplicateTypesInUnion {
                duplicate_name: expr.node.name().to_string(),
            },
            Range::from(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // We want to delete the "|" character as well as the duplicate
            // value, so delete from the end of "previous" to the end of "expr"
            diagnostic.set_fix(Edit::deletion(
                // it is impossible to have a duplicate without a "previous" node
                previous.unwrap().end_location.unwrap(),
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
    seen_nodes
}
