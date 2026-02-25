use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::pep_604_union;
use ruff_python_ast::{Expr, ExprBinOp, Operator};
use ruff_python_semantic::SemanticModel;
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

/// Returns `true` if a union expression contains nested sub-unions that would
/// need to be flattened by a fix.
///
/// For PEP 604 unions (`a | b | c`), the AST is left-recursive: `(a | b) | c`.
/// Only right-hand unions indicate actual nesting from parenthesization, e.g.
/// `a | (b | c)`. For `typing.Union`, any tuple element that is itself a union
/// is considered nested.
fn has_nested_union(semantic: &SemanticModel, expr: &Expr) -> bool {
    match expr {
        Expr::BinOp(ExprBinOp {
            op: Operator::BitOr,
            left,
            right,
            ..
        }) => is_union_expr(semantic, right) || has_nested_union(semantic, left),
        Expr::Subscript(subscript) if semantic.match_typing_expr(&subscript.value, "Union") => {
            if let Expr::Tuple(tuple) = &*subscript.slice {
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
fn is_union_expr(semantic: &SemanticModel, expr: &Expr) -> bool {
    match expr {
        Expr::BinOp(ExprBinOp {
            op: Operator::BitOr,
            ..
        }) => true,
        Expr::Subscript(subscript) => semantic.match_typing_expr(&subscript.value, "Union"),
        _ => false,
    }
}

/// RUF036
pub(crate) fn none_not_at_end_of_union<'a>(checker: &Checker, union: &'a Expr) {
    let semantic = checker.semantic();
    let mut none_exprs: Vec<&Expr> = Vec::new();
    let mut other_exprs: Vec<&Expr> = Vec::new();

    let mut last_expr: Option<&Expr> = None;
    let mut is_pep604 = false;

    let mut collect_members = |expr: &'a Expr, parent: &'a Expr| {
        if !is_pep604 {
            is_pep604 = matches!(parent, Expr::BinOp(_));
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
    if !has_nested_union(semantic, union) && !other_exprs.is_empty() {
        let nodes: Vec<&Expr> = other_exprs.iter().chain(&none_exprs).copied().collect();
        if let Some(fix) = generate_fix(checker, nodes, union, is_pep604) {
            diagnostic.set_fix(fix);
        }
    }
}

fn generate_fix(
    checker: &Checker,
    nodes: Vec<&Expr>,
    annotation: &Expr,
    is_pep604: bool,
) -> Option<Fix> {
    let applicability = if checker.comment_ranges().intersects(annotation.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    let reordered: Vec<Expr> = nodes.into_iter().cloned().collect();

    let new_expr = if is_pep604 {
        pep_604_union(&reordered)
    } else {
        // Preserve the original subscript value (e.g., `Union`, `U`, `typing.Union`).
        let Expr::Subscript(subscript) = annotation else {
            return None;
        };
        Expr::Subscript(ruff_python_ast::ExprSubscript {
            value: subscript.value.clone(),
            slice: Box::new(Expr::Tuple(ruff_python_ast::ExprTuple {
                elts: reordered,
                ctx: ruff_python_ast::ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                parenthesized: false,
            })),
            ctx: ruff_python_ast::ExprContext::Load,
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        })
    };

    let edit = Edit::range_replacement(checker.generator().expr(&new_expr), annotation.range());
    Some(Fix::applicable_edit(edit, applicability))
}
