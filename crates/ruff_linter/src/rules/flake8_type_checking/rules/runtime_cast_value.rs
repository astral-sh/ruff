use ruff_python_ast::Expr;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_type_checking::helpers::quote_type_expression;

/// ## What it does
/// Checks for an unquoted type expression in `typing.cast()` calls.
///
/// ## Why is this bad?
/// `typing.cast()` does not do anything at runtime, so the time spent
/// on evaluating the type expression is wasted.
///
/// In order to provide a consistent experience and keep this rule simple
/// type expressions will be quoted, even if they're so simple, that their
/// overhead becomes negligible (e.g. a single builtin name lookup like `str`).
///
/// This has the added benefit of making the type expression visually
/// distinct from the value expression, making it easier to see at a glance
/// where one ends and the other begins.
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
pub(crate) fn runtime_cast_value(checker: &mut Checker, type_expr: &Expr) {
    if type_expr.is_string_literal_expr() {
        return;
    }

    let mut diagnostic = Diagnostic::new(RuntimeCastValue, type_expr.range());
    let edit = quote_type_expression(
        type_expr,
        checker.semantic(),
        checker.stylist(),
        checker.locator(),
    );
    if checker
        .comment_ranges()
        .has_comments(type_expr, checker.source())
    {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
    checker.diagnostics.push(diagnostic);
}
