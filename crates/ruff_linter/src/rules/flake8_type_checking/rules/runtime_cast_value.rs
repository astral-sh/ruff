use ruff_python_ast::Expr;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_type_checking::helpers::quote_type_expression;

/// ## What it does
/// Checks for unquoted type expressions in `typing.cast()` calls.
///
/// ## Why is this bad?
/// This rule helps enforce a consistent style across your codebase.
///
/// It's often necessary to quote the first argument passed to `cast()`,
/// as type expressions can involve forward references, or references
/// to symbols which are only imported in `typing.TYPE_CHECKING` blocks.
/// This can lead to a visual inconsistency across different `cast()` calls,
/// where some type expressions are quoted but others are not. By enabling
/// this rule, you ensure that all type expressions passed to `cast()` are
/// quoted, enforcing stylistic consistency across all of your `cast()` calls.
///
/// In some cases where `cast()` is used in a hot loop, this rule may also
/// help avoid overhead from repeatedly evaluating complex type expressions at
/// runtime.
///
/// ## Example
/// ```python
/// from typing import cast
///
/// x = cast(dict[str, int], foo)
/// ```
///
/// Use instead:
/// ```python
/// from typing import cast
///
/// x = cast("dict[str, int]", foo)
/// ```
///
/// ## Fix safety
/// This fix is safe as long as the type expression doesn't span multiple
/// lines and includes comments on any of the lines apart from the last one.
#[derive(ViolationMetadata)]
pub(crate) struct RuntimeCastValue;

impl AlwaysFixableViolation for RuntimeCastValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Add quotes to type expression in `typing.cast()`".to_string()
    }

    fn fix_title(&self) -> String {
        "Add quotes".to_string()
    }
}

/// TC006
pub(crate) fn runtime_cast_value(checker: &Checker, type_expr: &Expr) {
    if type_expr.is_string_literal_expr() {
        return;
    }

    let mut diagnostic = Diagnostic::new(RuntimeCastValue, type_expr.range());
    let edit = quote_type_expression(
        type_expr,
        checker.semantic(),
        checker.stylist(),
        checker.locator(),
        checker.default_string_flags(),
    );
    if checker.comment_ranges().intersects(type_expr.range()) {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
    checker.report_diagnostic(diagnostic);
}
