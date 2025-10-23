use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprAttribute, helpers::Truthiness};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{Parentheses, remove_argument};
use crate::{Applicability, Fix, FixAvailability, Violation};

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
///
/// This rule's fix is marked as safe, unless the `dict.get()` call contains comments between
/// arguments that will be deleted.
///
/// ## Fix availability
///
/// This rule's fix is unavailable in cases where invalid arguments are provided to `dict.get`. As
/// shown in the [documentation], `dict.get` takes two positional-only arguments, so invalid cases
/// are identified by the presence of more than two arguments or any keyword arguments.
///
/// [documentation]: https://docs.python.org/3.13/library/stdtypes.html#dict.get
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.8.5")]
pub(crate) struct FalsyDictGetFallback;

impl Violation for FalsyDictGetFallback {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid providing a falsy fallback to `dict.get()` in boolean test positions. The default fallback `None` is already falsy.".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove falsy fallback from `dict.get()`".to_string())
    }
}

/// RUF056
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

    let mut diagnostic = checker.report_diagnostic(FalsyDictGetFallback, fallback_arg.range());

    // All arguments to `dict.get` are positional-only.
    if !call.arguments.keywords.is_empty() {
        return;
    }

    // And there are only two of them, at most.
    if call.arguments.args.len() > 2 {
        return;
    }

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
            checker.comment_ranges(),
        )
        .map(|edit| Fix::applicable_edit(edit, applicability))
    });
}
