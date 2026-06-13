use ruff_python_ast::Expr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_type_checking::helpers::quote_type_expression;
use crate::{AlwaysFixableViolation, Fix};

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
/// This fix is marked unsafe when the type expression contains a comment, since
/// quoting it would drop the comment.
///
/// The fix is also marked unsafe when quoting would introduce an escape sequence
/// (for example, `cast(Literal["'"], ...)`). Type checkers reject forward
/// references containing escape sequences.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.10.0")]
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

    let mut diagnostic = checker.report_diagnostic(RuntimeCastValue, type_expr.range());
    let edit = quote_type_expression(
        type_expr,
        checker.semantic(),
        checker.stylist(),
        checker.locator(),
        checker.default_string_flags(),
    );
    // A quoted forward reference containing an escape sequence (e.g. an inner quote
    // colliding with the outer quote) is rejected by type checkers, so mark it unsafe.
    let has_escape = edit.content().is_some_and(|content| content.contains('\\'));
    if has_escape || checker.comment_ranges().intersects(type_expr.range()) {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
}
