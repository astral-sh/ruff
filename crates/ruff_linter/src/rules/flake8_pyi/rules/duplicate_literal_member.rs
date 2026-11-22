use std::collections::HashSet;

use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{Builder, Expr, ExprKind};
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

/// ## What it does
/// Checks for duplicate members in a `typing.Literal[]` slice.
///
/// ## Why is this bad?
/// Duplicate literal members are redundant and should be removed.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// foo: Literal["a", "b", "a"]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// foo: Literal["a", "b"]
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the type annotation contains comments.
///
/// Note that while the fix may flatten nested literals into a single top-level literal,
/// the semantics of the annotation will remain unchanged.
///
/// ## References
/// - [Python documentation: `typing.Literal`](https://docs.python.org/3/library/typing.html#typing.Literal)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.6.0", category = Category::Suspicious)]
pub(crate) struct DuplicateLiteralMember {
    duplicate_name: String,
}

impl AlwaysFixableViolation for DuplicateLiteralMember {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Duplicate literal member `{}`", self.duplicate_name)
    }

    fn fix_title(&self) -> String {
        "Remove duplicates".to_string()
    }
}

/// PYI062
pub(crate) fn duplicate_literal_member<'a>(checker: &Checker, &expr: &Expr<'a>) {
    let mut seen_nodes: HashSet<ComparableExpr<'_>, _> = FxHashSet::default();
    let mut unique_nodes: Vec<Expr<'_>> = Vec::new();
    let mut diagnostics = Vec::new();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: Expr<'a>, _: Expr<'a>| {
        // If we've already seen this literal member, raise a violation.
        if seen_nodes.insert(expr.into()) {
            unique_nodes.push(expr);
        } else {
            diagnostics.push(checker.report_diagnostic(
                DuplicateLiteralMember {
                    duplicate_name: checker.generator().expr(expr),
                },
                expr.range(),
            ));
        }
    };

    // Traverse the literal, collect all diagnostic members.
    traverse_literal(&mut check_for_duplicate_members, checker.semantic(), expr);

    if diagnostics.is_empty() {
        return;
    }

    // If there's at least one diagnostic, create a fix to remove the duplicate members.
    if let ExprKind::Subscript(subscript) = expr.classify() {
        let mut builder = Builder::default();
        let value = builder.copy_node(subscript.value());
        let subscript = syntax::subscript_members(&mut builder, value, unique_nodes.into_iter());
        let fix = Fix::applicable_edit(
            Edit::range_replacement(
                checker.generator().expr(builder.get(subscript)),
                expr.range(),
            ),
            if checker.comment_ranges().intersects(expr.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        );
        for diagnostic in &mut diagnostics {
            diagnostic.set_fix(fix.clone());
        }
    }
}
