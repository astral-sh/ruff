use std::collections::HashSet;

use rustc_hash::FxHashSet;

use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Expr, ExprContext};
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::{Ranged, TextRange};

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
/// ## Fix safety
/// This rule's fix is marked as safe, unless the type annotation contains comments.
///
/// Note that while the fix may flatten nested literals into a single top-level literal,
/// the semantics of the annotation will remain unchanged.
///
/// ## References
/// - [Python documentation: `typing.Literal`](https://docs.python.org/3/library/typing.html#typing.Literal)
#[derive(ViolationMetadata)]
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
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: &'a Expr, _: &'a Expr| {
        // If we've already seen this literal member, raise a violation.
        if seen_nodes.insert(expr.into()) {
            unique_nodes.push(expr);
        } else {
            diagnostics.push(Diagnostic::new(
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
                    ctx: ExprContext::Load,
                    parenthesized: false,
                })
            }),
            value: subscript.value.clone(),
            range: TextRange::default(),
            ctx: ExprContext::Load,
        });
        let fix = Fix::applicable_edit(
            Edit::range_replacement(checker.generator().expr(&subscript), expr.range()),
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

    checker.report_diagnostics(diagnostics);
}
