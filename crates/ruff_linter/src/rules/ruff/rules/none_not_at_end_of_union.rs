use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Expr;
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
    #[derive_message_formats]
    fn message(&self) -> String {
        "`None` not at the end of the type annotation.".to_string()
    }
}

/// RUF036
pub(crate) fn none_not_at_end_of_union<'a>(checker: &Checker, union: &'a Expr) {
    let semantic = checker.semantic();
    let mut none_exprs: SmallVec<[&Expr; 1]> = SmallVec::new();

    let mut last_expr: Option<&Expr> = None;
    let mut find_none = |expr: &'a Expr, _parent: &Expr| {
        if matches!(expr, Expr::NoneLiteral(_)) {
            none_exprs.push(expr);
        }
        last_expr = Some(expr);
    };

    // Walk through all type expressions in the union and keep track of `None` literals.
    traverse_union(&mut find_none, semantic, union);

    let Some(last_expr) = last_expr else {
        return;
    };

    // The must be at least one `None` expression.
    let Some(last_none) = none_exprs.last() else {
        return;
    };

    // If any of the `None` literals is last we do not emit.
    if *last_none == last_expr {
        return;
    }

    for none_expr in none_exprs {
        checker.report_diagnostic(Diagnostic::new(NoneNotAtEndOfUnion, none_expr.range()));
    }
}
