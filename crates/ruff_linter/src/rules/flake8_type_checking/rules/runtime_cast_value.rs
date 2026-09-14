use ruff_python_ast::Expr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_type_checking::helpers::quote_type_expression;
use crate::{Fix, FixAvailability, Violation};

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
/// This rule's fix is marked as unsafe when the type expression contains a comment,
/// since quoting it would drop the comment.
///
/// No fix is offered when the type expression cannot be quoted without an escape
/// sequence, which a type checker rejects in a forward reference.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.10.0", category = Category::Pedantic)]
pub(crate) struct RuntimeCastValue;

impl Violation for RuntimeCastValue {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Add quotes to type expression in `typing.cast()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add quotes".to_string())
    }
}

/// TC006
pub(crate) fn runtime_cast_value(checker: &Checker, type_expr: &Expr) {
    if type_expr.is_string_literal_expr() {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(RuntimeCastValue, type_expr.range());
    let Some(edit) = quote_type_expression(
        type_expr,
        checker.semantic(),
        checker.stylist(),
        checker.locator(),
        checker.default_string_flags(),
        checker.target_version(),
    ) else {
        return;
    };
    // Quoting drops any comment inside the type expression.
    if checker.comment_ranges().intersects(type_expr.range()) {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
}
