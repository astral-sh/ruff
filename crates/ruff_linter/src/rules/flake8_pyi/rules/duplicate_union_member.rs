use std::collections::HashSet;

use anyhow::Result;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{Expr, ExprBinOp, ExprContext, ExprName, ExprSubscript, ExprTuple, Operator};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

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
    let mut unique_nodes: Vec<&Expr> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let mut union_type = UnionKind::TypingUnion;
    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut check_for_duplicate_members = |expr: &'a Expr, parent: &'a Expr| {
        if matches!(parent, Expr::BinOp(_)) {
            union_type = UnionKind::PEP604;
        }

        // If we've already seen this union member, raise a violation.
        if seen_nodes.insert(expr.into()) {
            unique_nodes.push(expr);
        } else {
            diagnostics.push(Diagnostic::new(
                DuplicateUnionMember {
                    duplicate_name: checker.generator().expr(expr),
                },
                expr.range(),
            ));
        }
    };

    // Traverse the union, collect all diagnostic members
    traverse_union(&mut check_for_duplicate_members, checker.semantic(), expr);

    if diagnostics.is_empty() {
        return;
    }

    if checker.settings.preview.is_enabled() {
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
                    generate_union_fix(checker, unique_nodes, expr, applicability).ok()
                }
            }
        };

        if let Some(fix) = fix {
            for diagnostic in &mut diagnostics {
                diagnostic.set_fix(fix.clone());
            }
        }
    }

    // Add all diagnostics to the checker
    checker.diagnostics.append(&mut diagnostics);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionKind {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    PEP604,
}

// Generate a [`Fix`] for two or more type expressions, e.g. `int | float | complex`.
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

// Generate a [`Fix`] for two or more type expresisons, e.g. `typing.Union[int, float, complex]`.
fn generate_union_fix(
    checker: &Checker,
    nodes: Vec<&Expr>,
    annotation: &Expr,
    applicability: Applicability,
) -> Result<Fix> {
    debug_assert!(nodes.len() >= 2, "At least two nodes required");

    // Request `typing.Union`
    let (import_edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import_from("typing", "Union"),
        annotation.start(),
        checker.semantic(),
    )?;

    // Construct the expression as `Subscript[typing.Union, Tuple[expr, [expr, ...]]]`
    let new_expr = Expr::Subscript(ExprSubscript {
        range: TextRange::default(),
        value: Box::new(Expr::Name(ExprName {
            id: Name::new(binding),
            ctx: ExprContext::Store,
            range: TextRange::default(),
        })),
        slice: Box::new(Expr::Tuple(ExprTuple {
            elts: nodes.into_iter().cloned().collect(),
            range: TextRange::default(),
            ctx: ExprContext::Load,
            parenthesized: false,
        })),
        ctx: ExprContext::Load,
    });

    Ok(Fix::applicable_edits(
        Edit::range_replacement(checker.generator().expr(&new_expr), annotation.range()),
        [import_edit],
        applicability,
    ))
}
