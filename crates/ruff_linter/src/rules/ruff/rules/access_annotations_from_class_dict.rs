use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprCall, ExprSubscript, PythonVersion};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `foo.__dict__.get("__annotations__")` or
/// `foo.__dict__["__annotations__"]` on Python 3.10+ and Python < 3.10 when
/// [typing-extensions](https://docs.astral.sh/ruff/settings/#lint_typing-extensions)
/// is enabled.
///
/// ## Why is this bad?
/// Starting with Python 3.14, directly accessing `__annotations__` via
/// `foo.__dict__.get("__annotations__")` or `foo.__dict__["__annotations__"]`
/// will only return annotations if the class is defined under
/// `from __future__ import annotations`.
///
/// Therefore, it is better to use dedicated library functions like
/// `annotationlib.get_annotations` (Python 3.14+), `inspect.get_annotations`
/// (Python 3.10+), or `typing_extensions.get_annotations` (for Python < 3.10 if
/// [typing-extensions](https://pypi.org/project/typing-extensions/) is
/// available).
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
/// foo.__dict__.get("__annotations__", {})
/// # or
/// foo.__dict__["__annotations__"]
/// ```
///
/// On Python 3.14+, use instead:
/// ```python
/// import annotationlib
///
/// annotationlib.get_annotations(foo)
/// ```
///
/// On Python 3.10+, use instead:
/// ```python
/// import inspect
///
/// inspect.get_annotations(foo)
/// ```
///
/// On Python < 3.10 with [typing-extensions](https://pypi.org/project/typing-extensions/)
/// installed, use instead:
/// ```python
/// import typing_extensions
///
/// typing_extensions.get_annotations(foo)
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
pub(crate) struct AccessAnnotationsFromClassDict {
    python_version: PythonVersion,
}

impl Violation for AccessAnnotationsFromClassDict {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let suggestion = if self.python_version >= PythonVersion::PY314 {
            "annotationlib.get_annotations"
        } else if self.python_version >= PythonVersion::PY310 {
            "inspect.get_annotations"
        } else {
            "typing_extensions.get_annotations"
        };
        format!("Use `{suggestion}` instead of `__dict__` access")
    }
}

/// RUF063
pub(crate) fn access_annotations_from_class_dict_with_get(checker: &Checker, call: &ExprCall) {
    let python_version = checker.target_version();
    let typing_extensions = checker.settings().typing_extensions;

    // Only apply this rule for Python 3.10 and newer unless `typing-extensions` is enabled.
    if python_version < PythonVersion::PY310 && !typing_extensions {
        return;
    }

    // Expected pattern: foo.__dict__.get("__annotations__" [, <default>])
    // Here, `call` is the `.get(...)` part.

    // 1. Check that the `call.func` is `get`
    let get_attribute = match call.func.as_ref() {
        Expr::Attribute(attr) if attr.attr.as_str() == "get" => attr,
        _ => return,
    };

    // 2. Check that the `get_attribute.value` is `__dict__`
    match get_attribute.value.as_ref() {
        Expr::Attribute(attr) if attr.attr.as_str() == "__dict__" => {}
        _ => return,
    }

    // At this point, we have `foo.__dict__.get`.

    // 3. Check arguments to `.get()`:
    //    - No keyword arguments.
    //    - One or two positional arguments.
    //    - First positional argument must be the string literal "__annotations__".
    //    - The value of the second positional argument (if present) does not affect the match.
    if !call.arguments.keywords.is_empty() || call.arguments.len() > 2 {
        return;
    }

    let Some(first_arg) = &call.arguments.find_positional(0) else {
        return;
    };

    let is_first_arg_correct = first_arg
        .as_string_literal_expr()
        .is_some_and(|s| s.value.to_str() == "__annotations__");

    if is_first_arg_correct {
        checker.report_diagnostic(
            AccessAnnotationsFromClassDict { python_version },
            call.range(),
        );
    }
}

/// RUF063
pub(crate) fn access_annotations_from_class_dict_by_key(
    checker: &Checker,
    subscript: &ExprSubscript,
) {
    let python_version = checker.target_version();
    let typing_extensions = checker.settings().typing_extensions;

    // Only apply this rule for Python 3.10 and newer unless `typing-extensions` is enabled.
    if python_version < PythonVersion::PY310 && !typing_extensions {
        return;
    }

    // Expected pattern: foo.__dict__["__annotations__"]

    // 1. Check that the slice is a string literal "__annotations__"
    if subscript
        .slice
        .as_string_literal_expr()
        .is_none_or(|s| s.value.to_str() != "__annotations__")
    {
        return;
    }

    // 2. Check that the `subscript.value` is `__dict__`
    let is_value_correct = subscript
        .value
        .as_attribute_expr()
        .is_some_and(|attr| attr.attr.as_str() == "__dict__");

    if is_value_correct {
        checker.report_diagnostic(
            AccessAnnotationsFromClassDict { python_version },
            subscript.range(),
        );
    }
}
