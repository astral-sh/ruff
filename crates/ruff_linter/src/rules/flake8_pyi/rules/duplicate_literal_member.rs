use std::collections::HashSet;

use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for duplicate members in a `typing.Literal[]` slice.
///
/// ## Why is this bad?
/// Duplicate literal members are redundant and should be removed.
///
/// ## Example
/// ```python
/// foo: Literal["a", "b", "a"]
/// ```
///
/// Use instead:
/// ```python
/// foo: Literal["a", "b"]
/// ```
///
/// ## References
/// - [Python documentation: `typing.Literal`](https://docs.python.org/3/library/typing.html#typing.Literal)
#[violation]
pub struct DuplicateLiteralMember {
    duplicate_name: String,
}

impl Violation for DuplicateLiteralMember {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Duplicate literal member `{}`", self.duplicate_name)
    }
}

/// PYI062
pub(crate) fn duplicate_literal_member<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut seen_nodes: HashSet<ComparableExpr<'_>, _> = FxHashSet::default();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: &'a Expr, _: &'a Expr| {
        // If we've already seen this literal member, raise a violation.
        if !seen_nodes.insert(expr.into()) {
            diagnostics.push(Diagnostic::new(
                DuplicateLiteralMember {
                    duplicate_name: checker.generator().expr(expr),
                },
                expr.range(),
            ));
        }
    };

    // Traverse the literal, collect all diagnostic members
    traverse_literal(&mut check_for_duplicate_members, checker.semantic(), expr);
    checker.diagnostics.append(&mut diagnostics);
}
