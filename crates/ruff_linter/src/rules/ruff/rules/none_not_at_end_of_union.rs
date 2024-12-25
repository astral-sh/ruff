use itertools::{Itertools, Position};
use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;
use smallvec::SmallVec;

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
        Some("Move `None` to the end of the type annotation".to_string())
    }
}

/// RUF036
pub(crate) fn none_not_at_end_of_union<'a>(checker: &mut Checker, union: &'a Expr) {
    let semantic = checker.semantic();
    let mut none_exprs: SmallVec<[&Expr; 1]> = SmallVec::new();
    let mut non_none_exprs: SmallVec<[&Expr; 1]> = SmallVec::new();
    let mut last_expr: Option<&Expr> = None;
    let mut find_none = |expr: &'a Expr, _parent: &Expr| {
        if matches!(expr, Expr::NoneLiteral(_)) {
            none_exprs.push(expr);
        } else {
            non_none_exprs.push(expr);
        }
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

    for (pos, none_expr) in none_exprs.iter().with_position() {
        let mut diagnostic = Diagnostic::new(NoneNotAtEndOfUnion, none_expr.range());
        if matches!(pos, Position::Last | Position::Only) {
            let mut elements = non_none_exprs
                .iter()
                .map(|expr| checker.locator().slice(expr.range()).to_string())
                .chain(std::iter::once("None".to_string()));
            let fix = if let Expr::Subscript(ast::ExprSubscript { slice, .. }) = union {
                let applicability = if checker.comment_ranges().intersects(slice.range()) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                };
                Fix::applicable_edit(
                    Edit::range_replacement(elements.join(", "), slice.range()),
                    applicability,
                )
            } else {
                let applicability = if checker.comment_ranges().intersects(union.range()) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                };
                Fix::applicable_edit(
                    Edit::range_replacement(elements.join(" | "), union.range()),
                    applicability,
                )
            };
            diagnostic.set_fix(fix);
        }

        checker.diagnostics.push(diagnostic);
    }
}
