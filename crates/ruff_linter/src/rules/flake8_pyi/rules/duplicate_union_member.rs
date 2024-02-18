use std::collections::HashSet;

use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for duplicate union members.
///
/// ## Why is this bad?
/// Duplicate union members are redundant and should be removed.
///
/// ## Example
/// ```python
/// foo: str | str
/// ```
///
/// Use instead:
/// ```python
/// foo: str
/// ```
///
/// ## References
/// - [Python documentation: `typing.Union`](https://docs.python.org/3/library/typing.html#typing.Union)
#[violation]
pub struct DuplicateUnionMember {
    duplicate_name: String,
}

impl Violation for DuplicateUnionMember {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Duplicate union member `{}`", self.duplicate_name)
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Remove duplicate union member `{}`",
            self.duplicate_name
        ))
    }
}

/// PYI016
pub(crate) fn duplicate_union_member<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut seen_nodes: HashSet<ComparableExpr<'_>, _> = FxHashSet::default();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: &'a Expr, parent: &'a Expr| {
        // If we've already seen this union member, raise a violation.
        if !seen_nodes.insert(expr.into()) {
            let mut diagnostic = Diagnostic::new(
                DuplicateUnionMember {
                    duplicate_name: checker.generator().expr(expr),
                },
                expr.range(),
            );
            // Delete the "|" character as well as the duplicate value by reconstructing the
            // parent without the duplicate.

            // If the parent node is not a `BinOp` we will not perform a fix
            if let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = parent {
                // Replace the parent with its non-duplicate child.
                let child = if expr == left.as_ref() { right } else { left };
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    checker.locator().slice(child.as_ref()).to_string(),
                    parent.range(),
                )));
            }
            diagnostics.push(diagnostic);
        }
    };

    // Traverse the union, collect all diagnostic members
    traverse_union(&mut check_for_duplicate_members, checker.semantic(), expr);

    // Add all diagnostics to the checker
    checker.diagnostics.append(&mut diagnostics);
}
