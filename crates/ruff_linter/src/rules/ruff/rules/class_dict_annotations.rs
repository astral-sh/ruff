use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprCall, PythonVersion};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `<identifier>.__dict__.get("__annotations__" [, <default>])`
/// on Python 3.10+ and Python < 3.10 with `typing_extensions` enabled.
///
/// ## Why is this bad?
/// Starting with Python 3.14, directly accessing `__annotations__` via
/// `<identifier>.__dict__.get("__annotations__")` will only return annotations
/// if the class is defined under `from __future__ import annotations`.
///
/// Therefore, it is better to use dedicated library functions like
/// `inspect.get_annotations` (Python 3.10+),
/// `typing_extensions.get_annotations` (for older Python versions if
/// `typing_extensions` is available), or `annotationlib.get_annotations`
/// (Python 3.14+).
///
/// The benefits of using these functions include:
/// 1.  **Avoiding Undocumented Internals:** They provide a stable, public API,
///     unlike direct `__dict__` access which relies on implementation details.
/// 2.  **Forward-Compatibility:** They are designed to handle changes in
///     Python's annotation system across versions, ensuring your code remains
///     robust (e.g., correctly handling the Python 3.14 behavior mentioned
///     above).
///
/// See [Python Annotations Best Practices](https://docs.python.org/3.14/howto/annotations.html)
/// for alternatives.
///
/// ## Example
///
/// ```python
/// cls.__dict__.get("__annotations__", {})
/// ```
///
/// On Python 3.14+, use instead:
/// ```python
/// import annotationlib
///
/// annotationlib.get_annotations(cls)
/// ```
///
/// On Python 3.10+, use instead:
/// ```python
/// import inspect
///
/// inspect.get_annotations(cls)
/// ```
///
/// On Python < 3.10 with `typing_extensions` enabled, use instead:
/// ```python
/// import typing_extensions
///
/// typing_extensions.get_annotations(cls)
/// ```
///
/// ## Fix safety
///
/// No autofix is currently provided for this rule.
///
/// ## Fix availability
///
/// No autofix is currently provided for this rule.
///
/// ## References
/// - [Python Annotations Best Practices](https://docs.python.org/3.14/howto/annotations.html)
#[derive(ViolationMetadata)]
pub(crate) struct ClassDictAnnotations;

impl Violation for ClassDictAnnotations {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `typing_extensions.get_annotations` (Python < 3.10 with \
        `typing_extensions` enabled), `inspect.get_annotations` \
        (Python 3.10+), or `annotationlib.get_annotations` (Python 3.14+) \
        instead of `__dict__.get('__annotations__')`"
            .to_string()
    }
}

/// RUF061
pub(crate) fn class_dict_annotations(checker: &Checker, call: &ExprCall) {
    // Only apply this rule for Python 3.10 and newer unless `typing_extensions` is enabled.
    if checker.target_version() < PythonVersion::PY310 && !checker.settings.typing_extensions {
        return;
    }

    // Expected pattern: <identifier>.__dict__.get("__annotations__" [, <default>])
    // Here, `call` is the `.get(...)` part.

    // 1. Check that the `call.func` is `get`
    let get_attribute = match call.func.as_ref() {
        Expr::Attribute(attr) if attr.attr.as_str() == "get" => attr,
        _ => return, // Not a call to an attribute named "get"
    };

    // 2. Check that the `get_attribute.value` is `__dict__`
    match get_attribute.value.as_ref() {
        Expr::Attribute(attr) if attr.attr.as_str() == "__dict__" => {}
        _ => return, // The object of ".get" is not an attribute named "__dict__"
    }

    // At this point, we have `<identifier>.__dict__.get`.

    // 3. Check arguments to `.get()`:
    //    - No keyword arguments.
    //    - One or two positional arguments.
    //    - First positional argument must be the string literal "__annotations__".
    //    - The value of the second positional argument (if present) does not affect the match.
    if call.arguments.keywords.is_empty()
        && (call.arguments.args.len() == 1 || call.arguments.args.len() == 2)
    {
        let first_arg = &call.arguments.args[0];
        let is_first_arg_correct = match first_arg.as_string_literal_expr() {
            Some(str_literal) => str_literal.value.to_str() == "__annotations__",
            None => false,
        };

        if is_first_arg_correct {
            // Pattern successfully matched! Report a diagnostic.
            let diagnostic = Diagnostic::new(ClassDictAnnotations, call.range());
            checker.report_diagnostic(diagnostic);
        }
    }
}
