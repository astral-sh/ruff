use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::helpers::pep_604_union;
use ruff_python_ast::{Expr, Operator};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::pad;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for type annotations where `None` is not at the end of a union.
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
#[violation_metadata(stable_since = "0.16.0", category = Category::Style)]
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

/// Returns `true` if a union expression contains nested sub-unions that would
/// need to be flattened by a fix.
///
/// For PEP 604 unions (`a | b | c`), the AST is left-recursive: `(a | b) | c`.
/// Only right-hand unions indicate actual nesting from parenthesization, e.g.
/// `a | (b | c)`. For `typing.Union`, any tuple element that is itself a union
/// is considered nested.
fn has_nested_union(semantic: &SemanticModel, expr: flat::View<'_, Expr>) -> bool {
    match expr.classify() {
        flat::ExprKind::BinOp(expr_bin_op) if expr_bin_op.op() == Operator::BitOr => {
            let left = expr_bin_op.left();
            let right = expr_bin_op.right();
            is_union_expr(semantic, right) || has_nested_union(semantic, left)
        }
        flat::ExprKind::Subscript(subscript)
            if semantic.match_typing_expr(subscript.value(), "Union") =>
        {
            if let flat::ExprKind::Tuple(tuple) = subscript.slice().classify() {
                tuple.iter().any(|elt| is_union_expr(semantic, elt))
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Returns `true` if `expr` is itself a union type (PEP 604 `|` or
/// `typing.Union[...]`).
fn is_union_expr(semantic: &SemanticModel, expr: flat::View<'_, Expr>) -> bool {
    match expr.classify() {
        flat::ExprKind::BinOp(expr_bin_op) if expr_bin_op.op() == Operator::BitOr => true,
        flat::ExprKind::Subscript(subscript) => {
            semantic.match_typing_expr(subscript.value(), "Union")
        }
        _ => false,
    }
}

/// RUF036
pub(crate) fn none_not_at_end_of_union<'a>(checker: &Checker, &union: &flat::View<'a, Expr>) {
    let semantic = checker.semantic();

    if !semantic.in_type_definition() {
        return;
    }

    let mut none_exprs: Vec<flat::View<'_, Expr>> = Vec::new();
    let mut other_exprs: Vec<flat::View<'_, Expr>> = Vec::new();

    let mut last_expr: Option<flat::View<'_, Expr>> = None;
    let mut is_pep604 = false;

    let mut collect_members = |expr: flat::View<'a, Expr>, parent: flat::View<'a, Expr>| {
        if !is_pep604 {
            is_pep604 = matches!(parent.classify(), flat::ExprKind::BinOp(_));
        }

        if matches!(expr.classify(), flat::ExprKind::NoneLiteral(_)) {
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

    // Skip fix for nested unions to avoid flattening, and for PEP 604 unions
    // with multiple `None`s to avoid generating `None | None`.
    if has_nested_union(semantic, union)
        || other_exprs.is_empty()
        || is_pep604 && none_exprs.len() > 1
    {
        return;
    }

    if let Some(fix) = generate_fix(checker, &other_exprs, &none_exprs, union, is_pep604) {
        diagnostic.set_fix(fix);
    }
}

fn generate_fix(
    checker: &Checker,
    other_exprs: &[flat::View<'_, Expr>],
    none_exprs: &[flat::View<'_, Expr>],
    annotation: flat::View<'_, Expr>,
    is_pep604: bool,
) -> Option<Fix> {
    let applicability = if checker.semantic().in_typing_only_annotation()
        && !checker.comment_ranges().intersects(annotation.range())
    {
        Applicability::Safe
    } else {
        Applicability::Unsafe
    };

    let reordered: Vec<Expr> = other_exprs
        .iter()
        .chain(none_exprs)
        .copied()
        .map(flat::View::to_owned_node)
        .collect();

    let new_expr = if is_pep604 {
        pep_604_union(reordered)
    } else {
        // Preserve the original subscript value (e.g., `Union`, `U`, `typing.Union`).
        let flat::ExprKind::Subscript(subscript) = annotation.classify() else {
            return None;
        };
        Expr::Subscript(ruff_python_ast::ExprSubscript {
            value: Box::new(subscript.value().to_owned_node()),
            slice: Box::new(Expr::Tuple(ruff_python_ast::ExprTuple {
                elts: reordered,
                ctx: ruff_python_ast::ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::NodeIndex::NONE,
                parenthesized: false,
            })),
            ctx: ruff_python_ast::ExprContext::Load,
            range: TextRange::default(),
            node_index: ruff_python_ast::NodeIndex::NONE,
        })
    };

    let edit = Edit::range_replacement(
        pad(
            checker.generator().expr(&new_expr),
            annotation.range(),
            checker.locator(),
        ),
        annotation.range(),
    );
    Some(Fix::applicable_edit(edit, applicability))
}
