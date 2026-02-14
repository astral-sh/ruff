use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprBinOp, Operator};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for type annotations where `None` is not at the end of an union.
///
/// ## Why is this bad?
/// Type annotation unions are commutative, meaning that the order of the elements
/// does not matter. The `None` literal represents the absence of a value. For
/// readability, it's preferred to write the more informative type expressions first.
///
/// ## Example
/// ```python
/// def func(arg: None | int): ...
/// ```
///
/// Use instead:
/// ```python
/// def func(arg: int | None): ...
/// ```
///
/// ## References
/// - [Python documentation: Union type](https://docs.python.org/3/library/stdtypes.html#types-union)
/// - [Python documentation: `typing.Optional`](https://docs.python.org/3/library/typing.html#typing.Optional)
/// - [Python documentation: `None`](https://docs.python.org/3/library/constants.html#None)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.7.4")]
pub(crate) struct NoneNotAtEndOfUnion;

impl Violation for NoneNotAtEndOfUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`None` not at the end of the type union.".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move `None` to the end of the type union".to_string())
    }
}

/// RUF036
pub(crate) fn none_not_at_end_of_union<'a>(checker: &Checker, union: &'a Expr) {
    let semantic = checker.semantic();
    let mut none_exprs: Vec<&Expr> = Vec::new();
    let mut other_exprs: Vec<&Expr> = Vec::new();

    let mut last_expr: Option<&Expr> = None;
    let mut first_parent: Option<&Expr> = None;
    let mut has_nested_union = false;
    let mut is_pep604 = false;

    let mut collect_members = |expr: &'a Expr, parent: &'a Expr| {
        // Detect nested unions by checking if the parent changes during traversal.
        match first_parent {
            None => {
                first_parent = Some(parent);
                is_pep604 = matches!(parent, Expr::BinOp(_));
            }
            Some(first) if !std::ptr::eq(first, parent) => {
                has_nested_union = true;
            }
            _ => {}
        }

        if matches!(expr, Expr::NoneLiteral(_)) {
            none_exprs.push(expr);
        } else {
            other_exprs.push(expr);
        }
        last_expr = Some(expr);
    };

    // Walk through all type expressions in the union and keep track of `None` literals.
    traverse_union(&mut collect_members, semantic, union);

    let Some(last_expr) = last_expr else {
        return;
    };

    // There must be at least one `None` expression.
    let Some(last_none) = none_exprs.last() else {
        return;
    };

    // If any of the `None` literals is last we do not emit.
    if *last_none == last_expr {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(NoneNotAtEndOfUnion, union.range());

    // Skip fix for nested unions to avoid flattening.
    if !has_nested_union
        && let Some(fix) = generate_fix(checker, &other_exprs, &none_exprs, union, is_pep604)
    {
        diagnostic.set_fix(fix);
    }
}

fn generate_fix(
    checker: &Checker,
    other_exprs: &[&Expr],
    none_exprs: &[&Expr],
    annotation: &Expr,
    is_pep604: bool,
) -> Option<Fix> {
    if other_exprs.is_empty() {
        return None;
    }

    let applicability = if checker.comment_ranges().intersects(annotation.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    let reordered: Vec<&Expr> = [other_exprs, none_exprs].concat();

    let edit = if is_pep604 {
        generate_pep604_fix(checker, reordered, annotation)
    } else {
        generate_typing_union_fix(checker, reordered, annotation)?
    };

    Some(Fix::applicable_edit(edit, applicability))
}

fn generate_pep604_fix(checker: &Checker, nodes: Vec<&Expr>, annotation: &Expr) -> Edit {
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

    Edit::range_replacement(checker.generator().expr(&new_expr), annotation.range())
}

fn generate_typing_union_fix(
    checker: &Checker,
    nodes: Vec<&Expr>,
    annotation: &Expr,
) -> Option<Edit> {
    let Expr::Subscript(subscript) = annotation else {
        return None;
    };

    let new_expr = Expr::Subscript(ruff_python_ast::ExprSubscript {
        value: subscript.value.clone(),
        slice: Box::new(Expr::Tuple(ruff_python_ast::ExprTuple {
            elts: nodes.into_iter().cloned().collect(),
            ctx: ruff_python_ast::ExprContext::Load,
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            parenthesized: false,
        })),
        ctx: ruff_python_ast::ExprContext::Load,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    });

    Some(Edit::range_replacement(
        checker.generator().expr(&new_expr),
        annotation.range(),
    ))
}
