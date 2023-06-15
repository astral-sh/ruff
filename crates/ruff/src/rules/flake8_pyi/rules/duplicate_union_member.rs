use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Operator, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct DuplicateUnionMember {
    duplicate_name: String,
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
pub(crate) fn duplicate_union_member(checker: &mut Checker, expr: &Expr) {
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
    if let Expr::BinOp(ast::ExprBinOp {
        op: Operator::BitOr,
        left,
        right,
        range: _,
    }) = expr
    {
        // Traverse left subtree, then the right subtree, propagating the previous node.
        traverse_union(seen_nodes, checker, left, Some(expr));
        traverse_union(seen_nodes, checker, right, Some(expr));
    }

    // If we've already seen this union member, raise a violation.
    if !seen_nodes.insert(expr.into()) {
        let mut diagnostic = Diagnostic::new(
            DuplicateUnionMember {
                duplicate_name: checker.generator().expr(expr),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // Delete the "|" character as well as the duplicate value by reconstructing the
            // parent without the duplicate.

            // SAFETY: impossible to have a duplicate without a `parent` node.
            let parent = parent.expect("Parent node must exist");

            // SAFETY: Parent node must have been a `BinOp` in order for us to have traversed it.
            let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = parent else {
                panic!("Parent node must be a BinOp");
            };

            // Replace the parent with its non-duplicate child.
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                checker
                    .generator()
                    .expr(if expr == left.as_ref() { right } else { left }),
                parent.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
