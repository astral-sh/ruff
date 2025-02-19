use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprSubscript, PythonVersion};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `Unpack[]` on Python 3.11 and above, and suggests
/// using `*` instead.
///
/// ## Why is this bad?
/// [PEP 646] introduced a new syntax for unpacking sequences based on the `*`
/// operator. This syntax is more concise and readable than the previous
/// `Unpack[]` syntax.
///
/// ## Example
/// ```python
/// from typing import Unpack
///
///
/// def foo(*args: Unpack[tuple[int, ...]]) -> None:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def foo(*args: *tuple[int, ...]) -> None:
///     pass
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as `Unpack[T]` and `*T` are considered
/// different values when introspecting types at runtime. However, in most cases,
/// the fix should be safe to apply.
///
/// [PEP 646]: https://peps.python.org/pep-0646/
#[derive(ViolationMetadata)]
pub(crate) struct NonPEP646Unpack;

impl Violation for NonPEP646Unpack {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `*` for unpacking".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to `*` for unpacking".to_string())
    }
}

/// UP044
pub(crate) fn use_pep646_unpack(checker: &Checker, expr: &ExprSubscript) {
    if checker.target_version() < PythonVersion::PY311 {
        return;
    }

    if !checker.semantic().seen_typing() {
        return;
    }

    let ExprSubscript {
        range,
        value,
        slice,
        ..
    } = expr;

    // Skip semantically invalid subscript calls (e.g. `Unpack[str | num]`).
    if !(slice.is_name_expr() || slice.is_subscript_expr() || slice.is_attribute_expr()) {
        return;
    }

    if !checker.semantic().match_typing_expr(value, "Unpack") {
        return;
    }

    // Determine whether we're in a valid context for a star expression.
    //
    // Star expressions are only allowed in two places:
    // - Subscript indexes (e.g., `Generic[DType, *Shape]`).
    // - Variadic positional arguments (e.g., `def f(*args: *int)`).
    //
    // See: <https://peps.python.org/pep-0646/#grammar-changes>
    if !in_subscript_index(expr, checker.semantic()) && !in_vararg(expr, checker.semantic()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(NonPEP646Unpack, *range);
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        format!("*{}", checker.locator().slice(slice.as_ref())),
        *range,
    )));
    checker.report_diagnostic(diagnostic);
}

/// Determine whether the [`ExprSubscript`] is in a subscript index (e.g., `Generic[Unpack[int]]`).
fn in_subscript_index(expr: &ExprSubscript, semantic: &SemanticModel) -> bool {
    let parent = semantic
        .current_expressions()
        .skip(1)
        .find_map(|expr| expr.as_subscript_expr());

    let Some(parent) = parent else {
        return false;
    };

    // E.g., `Generic[Unpack[int]]`.
    if parent
        .slice
        .as_subscript_expr()
        .is_some_and(|slice| slice == expr)
    {
        return true;
    }

    // E.g., `Generic[DType, Unpack[int]]`.
    if parent.slice.as_tuple_expr().is_some_and(|slice| {
        slice
            .elts
            .iter()
            .any(|elt| elt.as_subscript_expr().is_some_and(|elt| elt == expr))
    }) {
        return true;
    }

    false
}

/// Determine whether the [`ExprSubscript`] is attached to a variadic argument in a function
/// definition (e.g., `def f(*args: Unpack[int])`).
fn in_vararg(expr: &ExprSubscript, semantic: &SemanticModel) -> bool {
    let parent = semantic.current_statement().as_function_def_stmt();

    let Some(parent) = parent else {
        return false;
    };

    parent
        .parameters
        .vararg
        .as_ref()
        .and_then(|vararg| vararg.annotation())
        .and_then(Expr::as_subscript_expr)
        == Some(expr)
}
