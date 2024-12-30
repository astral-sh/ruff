use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{helpers::Truthiness, Expr, ExprAttribute, ExprCall, ExprName};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

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

pub(crate) fn falsy_dict_get_fallback(checker: &mut Checker, expr: &Expr) {
    let semantic = checker.semantic();

    // Check if the expression is a call
    let Expr::Call(ExprCall { func, .. }) = expr else {
        return;
    };

    // Check if the function being called is an attribute (e.g. `dict.get`)
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = &**func else {
        return;
    };

    // Ensure the method called is `get`
    if attr != "get" {
        return;
    }

    // Check if we are in a boolean test
    if !semantic.in_boolean_test() {
        return;
    }

    // Check if the object is a dictionary using the semantic model
    if let Expr::Name(expr_name) = &**value {
        if !is_known_to_be_of_type_dict(semantic, expr_name) {
            return;
        }
    } else {
        return;
    }

    let Expr::Call(ExprCall { arguments, .. }) = expr else {
        return;
    };

    // Get the fallback argument
    let Some(fallback_arg) = arguments.find_argument("default", 1) else {
        return;
    };

    // Check if the fallback is a falsy value
    let is_falsy = matches!(
        Truthiness::from_expr(fallback_arg, |id| semantic.has_builtin_binding(id)),
        Truthiness::Falsey | Truthiness::False | Truthiness::None
    );

    if !is_falsy {
        return;
    }

    let mut diagnostic = Diagnostic::new(FalsyDictGetFallback, fallback_arg.range());

    let key_arg = arguments.find_argument("key", 0).unwrap();
    let comment_ranges = checker.comment_ranges();

    let Some(full_fallback_arg) = arguments.find_keyword("default") else {
        // Fallback not specified as a keyword.

        // Determine applicability based on the presence of comments
        let applicability = if comment_ranges.intersects(TextRange::new(
            key_arg.range().end(),
            fallback_arg.range().end(),
        )) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };
        diagnostic.try_set_fix(|| {
            remove_argument(
                fallback_arg,
                arguments,
                Parentheses::Preserve,
                checker.locator().contents(),
            )
            .map(|edit| Fix::applicable_edit(edit, applicability))
        });
        checker.diagnostics.push(diagnostic);
        return;
    };
    // Fallback is specified as a keyword.
    // Get range for the fallback argument (else clause handling case where args are supplied out of positional order)
    let (range_start, range_end) = if key_arg.range().end() <= full_fallback_arg.range().end() {
        (key_arg.range().end(), full_fallback_arg.range().end())
    } else {
        (full_fallback_arg.range().start(), key_arg.range().start())
    };

    // Determine applicability based on the presence of comments
    let applicability = if comment_ranges.intersects(TextRange::new(range_start, range_end)) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    diagnostic.try_set_fix(|| {
        remove_argument(
            full_fallback_arg,
            arguments,
            Parentheses::Preserve,
            checker.locator().contents(),
        )
        .map(|edit| Fix::applicable_edit(edit, applicability))
    });

    checker.diagnostics.push(diagnostic);
}

fn is_known_to_be_of_type_dict(semantic: &SemanticModel, expr: &ExprName) -> bool {
    let Some(binding) = semantic.only_binding(expr).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_dict(binding, semantic)
}
