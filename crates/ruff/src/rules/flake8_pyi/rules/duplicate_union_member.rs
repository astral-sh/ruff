use rustc_hash::FxHashSet;
use rustpython_parser::ast::{Expr, ExprKind, Operator};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct DuplicateUnionMember {
    pub duplicate_name: String,
}

impl AlwaysAutofixableViolation for DuplicateUnionMember {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Duplicate union member `{}`", self.duplicate_name)
    }

    fn autofix_title(&self) -> String {
        format!("Remove duplicate union member `{}`", self.duplicate_name)
    }
}

/// PYI016
pub fn duplicate_union_member(checker: &mut Checker, expr: &Expr) {
    let mut seen_nodes = FxHashSet::default();
    traverse_union(&mut seen_nodes, checker, expr, None);
}

fn traverse_union<'a>(
    seen_nodes: &mut FxHashSet<ComparableExpr<'a>>,
    checker: &mut Checker,
    expr: &'a Expr,
    parent: Option<&'a Expr>,
) {
    // The union data structure usually looks like this:
    //  a | b | c -> (a | b) | c
    //
    // However, parenthesized expressions can coerce it into any structure:
    //  a | (b | c)
    //
    // So we have to traverse both branches in order (left, then right), to report duplicates
    // in the order they appear in the source code.
    if let ExprKind::BinOp {
        op: Operator::BitOr,
        left,
        right,
    } = &expr.node
    {
        // Traverse left subtree, then the right subtree, propagating the previous node.
        traverse_union(seen_nodes, checker, left, Some(expr));
        traverse_union(seen_nodes, checker, right, Some(expr));
    }

    // If we've already seen this union member, raise a violation.
    if !seen_nodes.insert(expr.into()) {
        let mut diagnostic = Diagnostic::new(
            DuplicateUnionMember {
                duplicate_name: unparse_expr(expr, checker.stylist),
            },
            Range::from(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // Delete the "|" character as well as the duplicate value by reconstructing the
            // parent without the duplicate.

            // SAFETY: impossible to have a duplicate without a `parent` node.
            let parent = parent.expect("Parent node must exist");

            // SAFETY: Parent node must have been a `BinOp` in order for us to have traversed it.
            let ExprKind::BinOp { left, right, .. } = &parent.node else {
                panic!("Parent node must be a BinOp");
            };

            // Replace the parent with its non-duplicate child.
            diagnostic.set_fix(Edit::replacement(
                unparse_expr(
                    if expr.node == left.node { right } else { left },
                    checker.stylist,
                ),
                parent.location,
                parent.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
