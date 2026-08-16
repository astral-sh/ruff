use std::collections::HashSet;

use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Expr, ExprContext, helpers::any_over_expr};
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
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
/// This rule's fix is marked as safe, unless the type annotation contains comments, or a removed
/// duplicate is a dynamic expression (e.g. `Literal[items[0], items[0]]`) whose repeated
/// evaluation could have side effects.
///
/// Note that while the fix may flatten nested literals into a single top-level literal,
/// the semantics of the annotation will remain unchanged.
///
/// ## References
/// - [Python documentation: `typing.Literal`](https://docs.python.org/3/library/typing.html#typing.Literal)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.6.0")]
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
pub(crate) fn duplicate_literal_member<'a>(checker: &Checker, expr: &'a Expr) {
    let mut seen_nodes: HashSet<ComparableExpr<'_>, _> = FxHashSet::default();
    let mut unique_nodes: Vec<&Expr> = Vec::new();
    let mut diagnostics = Vec::new();
    // Whether any *removed* duplicate is a dynamic expression (e.g. `items[0]`) whose repeated
    // evaluation may have side effects. Dropping such a member changes runtime behavior, so the
    // fix must then be marked unsafe.
    let mut has_dynamic_duplicate = false;

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: &'a Expr, _: &'a Expr| {
        // If we've already seen this literal member, raise a violation.
        if seen_nodes.insert(expr.into()) {
            unique_nodes.push(expr);
        } else {
            if duplicate_removal_is_unsafe(expr) {
                has_dynamic_duplicate = true;
            }
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
    if let Expr::Subscript(subscript) = expr {
        let subscript = Expr::Subscript(ast::ExprSubscript {
            slice: Box::new(if let [elt] = unique_nodes.as_slice() {
                (*elt).clone()
            } else {
                Expr::Tuple(ast::ExprTuple {
                    elts: unique_nodes.into_iter().cloned().collect(),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                    ctx: ExprContext::Load,
                    parenthesized: false,
                })
            }),
            value: subscript.value.clone(),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            ctx: ExprContext::Load,
        });
        let fix = Fix::applicable_edit(
            Edit::range_replacement(checker.generator().expr(&subscript), expr.range()),
            if checker.comment_ranges().intersects(expr.range()) || has_dynamic_duplicate {
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

/// Returns `true` if removing a duplicate occurrence of `expr` could change runtime behavior
/// because evaluating it has observable side effects.
///
/// Well-formed `Literal` members — literals, dotted enum-member names, and displays of those — are
/// side-effect-free to re-evaluate. A call or subscript, however (e.g. `items[0]` or `foo()`, only
/// reachable for a syntactically-invalid `Literal`), invokes `__call__`/`__getitem__`, which may
/// have side effects or return a different value each time, so dropping a "duplicate" is unsafe.
fn duplicate_removal_is_unsafe(expr: &Expr) -> bool {
    any_over_expr(expr, |expr| {
        matches!(expr, Expr::Call(_) | Expr::Subscript(_))
    })
}
