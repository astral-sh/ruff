use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{helpers::Truthiness, Expr, ExprAttribute};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for `dict.get(key, falsy_value)` calls in boolean test positions.
///
/// ## Why is this bad?
/// The default fallback `None` is already falsy.
///
/// ## Example
///
/// ```python
/// if dict.get(key, False):
///     ...
/// ```
///
/// Use instead:
///
/// ```python
/// if dict.get(key):
///     ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the `dict.get()` call contains comments between arguments.
#[derive(ViolationMetadata)]
pub(crate) struct FalsyDictGetFallback;

impl AlwaysFixableViolation for FalsyDictGetFallback {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid providing a falsy fallback to `dict.get()` in boolean test positions. The default fallback `None` is already falsy.".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove falsy fallback from `dict.get()`".to_string()
    }
}

pub(crate) fn falsy_dict_get_fallback(checker: &Checker, expr: &Expr) {
    let semantic = checker.semantic();

    // Check if we are in a boolean test
    if !semantic.in_boolean_test() {
        return;
    }

    // Check if the expression is a call
    let Expr::Call(call) = expr else {
        return;
    };

    // Check if the function being called is an attribute (e.g. `dict.get`)
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = &*call.func else {
        return;
    };

    // Ensure the method called is `get`
    if attr != "get" {
        return;
    }

    // Check if the object is a dictionary using the semantic model
    if !value
        .as_name_expr()
        .is_some_and(|name| typing::is_known_to_be_of_type_dict(semantic, name))
    {
        return;
    }

    // Get the fallback argument
    let Some(fallback_arg) = call.arguments.find_argument("default", 1) else {
        return;
    };

    // Check if the fallback is a falsy value
    if Truthiness::from_expr(fallback_arg.value(), |id| semantic.has_builtin_binding(id))
        .into_bool()
        != Some(false)
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(FalsyDictGetFallback, fallback_arg.range());

    let comment_ranges = checker.comment_ranges();

    // Determine applicability based on the presence of comments
    let applicability = if comment_ranges.intersects(call.arguments.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    diagnostic.try_set_fix(|| {
        remove_argument(
            &fallback_arg,
            &call.arguments,
            Parentheses::Preserve,
            checker.locator().contents(),
        )
        .map(|edit| Fix::applicable_edit(edit, applicability))
    });

    checker.report_diagnostic(diagnostic);
}
