use ruff_python_ast::{BoolOp, Expr};
use ruff_text_size::Ranged;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::rules::flake8_simplify::helpers::{ContentAround, is_short_circuit};
use crate::{AlwaysFixableViolation, Fix};
/// ## What it does
/// Checks for `or` expressions that contain truthy values.
///
/// ## Why is this bad?
/// If the expression is used as a condition, it can be replaced in-full with
/// `True`.
///
/// In other cases, the expression can be short-circuited to the first truthy
/// value.
///
/// By using `True` (or the first truthy value), the code is more concise
/// and easier to understand, since it no longer contains redundant conditions.
///
/// ## Example
/// ```python
/// if x or [1] or y:
///     pass
///
/// a = x or [1] or y
/// ```
///
/// Use instead:
/// ```python
/// if True:
///     pass
///
/// a = x or [1]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ExprOrTrue {
    expr: String,
    remove: ContentAround,
}

impl AlwaysFixableViolation for ExprOrTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprOrTrue { expr, remove } = self;
        let replaced = match remove {
            ContentAround::After => format!("{expr} or ..."),
            ContentAround::Before => format!("... or {expr}"),
            ContentAround::Both => format!("... or {expr} or ..."),
        };
        format!("Use `{expr}` instead of `{replaced}`")
    }

    fn fix_title(&self) -> String {
        let ExprOrTrue { expr, .. } = self;
        format!("Replace with `{expr}`")
    }
}

/// SIM222
pub(crate) fn expr_or_true(checker: &Checker, expr: &Expr) {
    if checker.semantic().in_string_type_definition() {
        return;
    }

    if let Some((edit, remove)) = is_short_circuit(expr, BoolOp::Or, checker) {
        let mut diagnostic = checker.report_diagnostic(
            ExprOrTrue {
                expr: edit.content().unwrap_or_default().to_string(),
                remove,
            },
            edit.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    }
}
