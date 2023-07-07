use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Ranged};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_pyi::helpers::traverse_union;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;

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
pub(crate) fn duplicate_union_member<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut seen_nodes: std::collections::HashSet<ComparableExpr<'_>, _> = FxHashSet::default();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: &'a Expr, parent: Option<&'a Expr>| {
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
            diagnostics.push(diagnostic);
        }
    };

    // Traverse the union, collect all diagnostic members
    traverse_union(
        &mut check_for_duplicate_members,
        checker.semantic(),
        expr,
        None,
    );

    // Add all diagnostics to the checker
    checker.diagnostics.append(&mut diagnostics);
}
