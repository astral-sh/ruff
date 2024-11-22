use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
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
#[violation]
pub struct RuntimeCastValue;

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
pub(crate) fn runtime_cast_value(checker: &mut Checker, type_expr: &Expr) {
    if type_expr.is_string_literal_expr() {
        return;
    }

    let mut diagnostic = Diagnostic::new(RuntimeCastValue, type_expr.range());
    let edit = quote_type_expression(type_expr, checker.semantic(), checker.stylist()).ok();
    if let Some(edit) = edit {
        if checker
            .comment_ranges()
            .has_comments(type_expr, checker.source())
        {
            diagnostic.set_fix(Fix::unsafe_edit(edit));
        } else {
            diagnostic.set_fix(Fix::safe_edit(edit));
        }
    }
    checker.diagnostics.push(diagnostic);
}
