use ruff_python_ast::{BoolOp, Expr};
use ruff_text_size::Ranged;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::rules::flake8_simplify::helpers::{is_short_circuit, ContentAround};
use crate::{AlwaysFixableViolation, Fix};
/// ## What it does
/// Checks for `and` expressions that contain falsey values.
///
/// ## Why is this bad?
/// If the expression is used as a condition, it can be replaced in-full with
/// `False`.
///
/// In other cases, the expression can be short-circuited to the first falsey
/// value.
///
/// By using `False` (or the first falsey value), the code is more concise
/// and easier to understand, since it no longer contains redundant conditions.
///
/// ## Example
/// ```python
/// if x and [] and y:
///     pass
///
/// a = x and [] and y
/// ```
///
/// Use instead:
/// ```python
/// if False:
///     pass
///
/// a = x and []
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ExprAndFalse {
    expr: String,
    remove: ContentAround,
}

impl AlwaysFixableViolation for ExprAndFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprAndFalse { expr, remove } = self;
        let replaced = match remove {
            ContentAround::After => format!(r#"{expr} and ..."#),
            ContentAround::Before => format!("... and {expr}"),
            ContentAround::Both => format!("... and {expr} and ..."),
        };
        format!("Use `{expr}` instead of `{replaced}`")
    }

    fn fix_title(&self) -> String {
        let ExprAndFalse { expr, .. } = self;
        format!("Replace with `{expr}`")
    }
}


/// SIM223
pub(crate) fn expr_and_false(checker: &Checker, expr: &Expr) {
    if checker.semantic().in_string_type_definition() {
        return;
    }

    if let Some((edit, remove)) = is_short_circuit(expr, BoolOp::And, checker) {
        let mut diagnostic = checker.report_diagnostic(
            ExprAndFalse {
                expr: edit.content().unwrap_or_default().to_string(),
                remove,
            },
            edit.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    }
}
