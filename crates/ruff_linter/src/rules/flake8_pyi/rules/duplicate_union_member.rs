use rustc_hash::FxHashSet;
use std::collections::HashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{AtomicNodeIndex, Expr, ExprBinOp, ExprNoneLiteral, Operator, PythonVersion};
use ruff_python_semantic::analyze::typing::{traverse_union, traverse_union_and_optional};
use ruff_text_size::{Ranged, TextRange, TextSize};

use super::generate_union_fix;
use crate::checkers::ast::Checker;
use crate::preview::is_optional_as_none_in_union_enabled;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
/// This rule's fix is marked as safe unless the union contains comments.
///
/// For nested union, the fix will flatten type expressions into a single
/// top-level union.
///
/// ## References
/// - [Python documentation: `typing.Union`](https://docs.python.org/3/library/typing.html#typing.Union)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.262")]
pub(crate) struct DuplicateUnionMember {
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
pub(crate) fn duplicate_union_member<'a>(checker: &Checker, expr: &'a Expr) {
    let mut seen_nodes: HashSet<ComparableExpr<'_>, _> = FxHashSet::default();
    let mut unique_nodes: Vec<&Expr> = Vec::new();
    let mut diagnostics = Vec::new();

    let mut union_type = UnionKind::TypingUnion;
    let mut optional_present = false;
    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: &'a Expr, parent: &'a Expr| {
        if matches!(parent, Expr::BinOp(_)) {
            union_type = UnionKind::PEP604;
        }

        let virtual_expr = if is_optional_as_none_in_union_enabled(checker.settings())
            && is_optional_type(checker, expr)
        {
            // If the union member is an `Optional`, add a virtual `None` literal.
            optional_present = true;
            &VIRTUAL_NONE_LITERAL
        } else {
            expr
        };

        // If we've already seen this union member, raise a violation.
        if seen_nodes.insert(virtual_expr.into()) {
            unique_nodes.push(virtual_expr);
        } else {
            diagnostics.push(checker.report_diagnostic(
                DuplicateUnionMember {
                    duplicate_name: checker.generator().expr(virtual_expr),
                },
                // Use the real expression's range for diagnostics.
                expr.range(),
            ));
        }
    };

    // Traverse the union, collect all diagnostic members
    if is_optional_as_none_in_union_enabled(checker.settings()) {
        traverse_union_and_optional(&mut check_for_duplicate_members, checker.semantic(), expr);
    } else {
        traverse_union(&mut check_for_duplicate_members, checker.semantic(), expr);
    }

    if diagnostics.is_empty() {
        return;
    }

    // Do not reduce `Union[None, ... None]` to avoid introducing a `TypeError` unintentionally
    // e.g. `isinstance(None, Union[None, None])`, if reduced to `isinstance(None, None)`, causes
    // `TypeError: isinstance() arg 2 must be a type, a tuple of types, or a union` to throw.
    if unique_nodes.iter().all(|expr| expr.is_none_literal_expr()) && !optional_present {
        return;
    }

    // Mark [`Fix`] as unsafe when comments are in range.
    let applicability = if checker.comment_ranges().intersects(expr.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    // Generate the flattened fix once.
    let fix = if let &[edit_expr] = unique_nodes.as_slice() {
        // Generate a [`Fix`] for a single type expression, e.g. `int`.
        Some(Fix::applicable_edit(
            Edit::range_replacement(checker.generator().expr(edit_expr), expr.range()),
            applicability,
        ))
    } else {
        match union_type {
            // See redundant numeric union
            UnionKind::PEP604 => Some(generate_pep604_fix(
                checker,
                unique_nodes,
                expr,
                applicability,
            )),
            UnionKind::TypingUnion => {
                // Request `typing.Union`
                let Some(importer) = checker.typing_importer("Union", PythonVersion::lowest())
                else {
                    return;
                };
                generate_union_fix(
                    checker.generator(),
                    &importer,
                    unique_nodes,
                    expr,
                    applicability,
                )
                .ok()
            }
        }
    };

    if let Some(fix) = fix {
        for diagnostic in &mut diagnostics {
            diagnostic.set_fix(fix.clone());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionKind {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    PEP604,
}

/// Generate a [`Fix`] for two or more type expressions, e.g. `int | float | complex`.
fn generate_pep604_fix(
    checker: &Checker,
    nodes: Vec<&Expr>,
    annotation: &Expr,
    applicability: Applicability,
) -> Fix {
    debug_assert!(nodes.len() >= 2, "At least two nodes required");

    let new_expr = nodes
        .into_iter()
        .fold(None, |acc: Option<Expr>, right: &Expr| {
            if let Some(left) = acc {
                Some(Expr::BinOp(ExprBinOp {
                    left: Box::new(left),
                    op: Operator::BitOr,
                    right: Box::new(right.clone()),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                }))
            } else {
                Some(right.clone())
            }
        })
        .unwrap();

    Fix::applicable_edit(
        Edit::range_replacement(checker.generator().expr(&new_expr), annotation.range()),
        applicability,
    )
}

static VIRTUAL_NONE_LITERAL: Expr = Expr::NoneLiteral(ExprNoneLiteral {
    node_index: AtomicNodeIndex::NONE,
    range: TextRange::new(TextSize::new(0), TextSize::new(0)),
});

fn is_optional_type(checker: &Checker, expr: &Expr) -> bool {
    checker.semantic().match_typing_expr(expr, "Optional")
}
