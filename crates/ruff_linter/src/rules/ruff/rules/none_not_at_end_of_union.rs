use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;
use smallvec::SmallVec;
use ruff_diagnostics::{Fix};
use crate::{Edit, FixAvailability};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for type annotations where `None` is not at the end of an union.
///
/// ## Why is this bad?
/// Type annotation unions are associative, meaning that the order of the elements
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
pub(crate) struct NoneNotAtEndOfUnion;

impl Violation for NoneNotAtEndOfUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`None` not at the end of the type annotation.".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move `None` to the end of the union".to_string())
    }
}

/// RUF036
pub(crate) fn none_not_at_end_of_union<'a>(checker: &Checker, union: &'a Expr) {
    let semantic = checker.semantic();
    let mut none_exprs: SmallVec<[&Expr; 1]> = SmallVec::new();
    let mut all_exprs: SmallVec<[&Expr; 4]> = SmallVec::new();
    let mut has_nested_union = false;

    let mut last_expr: Option<&Expr> = None;
    let mut find_none = |expr: &'a Expr, parent: &Expr| {
        // Detect nested unions: if the parent is not the top-level union, and is a union, mark as nested
        if parent != union {
            if matches!(parent, Expr::BinOp(_) | Expr::Subscript(_)) {
                has_nested_union = true;
            }
        }
        if matches!(expr, Expr::NoneLiteral(_)) {
            none_exprs.push(expr);
        }
        all_exprs.push(expr);
        last_expr = Some(expr);
    };

    // Walk through all type expressions in the union and keep track of `None` literals.
    traverse_union(&mut find_none, semantic, union);

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

    // Autofix only if there is exactly one None and no nested unions
    let can_fix = none_exprs.len() == 1 && !has_nested_union;

    for none_expr in none_exprs {
        let mut diagnostic = checker.report_diagnostic(NoneNotAtEndOfUnion, none_expr.range());
        if can_fix {
            // Build the fixed union string: move None to the end
            let locator = checker.locator();
            let mut union_ranges: Vec<_> = all_exprs.iter().map(ruff_text_size::Ranged::range).collect();
            // Remove the None's range
            let none_index = all_exprs.iter().position(|e| matches!(e, Expr::NoneLiteral(_))).unwrap();
            union_ranges.remove(none_index);
            // Remove None from exprs
            let mut expr_texts: Vec<_> = all_exprs.iter().map(|e| locator.slice(e.range())).collect();
            let none_text = expr_texts.remove(none_index);
            // Add None to the end
            expr_texts.push(none_text);
            // Reconstruct the union string
            let fixed = expr_texts.join(" | ");
            // Replace the whole union expression
            let fix = Fix::unsafe_edit(Edit::range_replacement(fixed, union.range()));
            diagnostic.set_fix(fix);
        }
    }
}
