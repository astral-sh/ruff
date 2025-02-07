use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility::is_overload;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for `@overload` function definitions that contain a docstring.
///
/// ## Why is this bad?
/// The `@overload` decorator is used to define multiple compatible signatures
/// for a given function, to support type-checking. A series of `@overload`
/// definitions should be followed by a single non-decorated definition that
/// contains the implementation of the function.
///
/// `@overload` function definitions should not contain a docstring; instead,
/// the docstring should be placed on the non-decorated definition that contains
/// the implementation.
///
/// ## Example
///
/// ```python
/// from typing import overload
///
///
/// @overload
/// def factorial(n: int) -> int:
///     """Return the factorial of n."""
///
///
/// @overload
/// def factorial(n: float) -> float:
///     """Return the factorial of n."""
///
///
/// def factorial(n):
///     """Return the factorial of n."""
///
///
/// factorial.__doc__  # "Return the factorial of n."
/// ```
///
/// Use instead:
///
/// ```python
/// from typing import overload
///
///
/// @overload
/// def factorial(n: int) -> int: ...
///
///
/// @overload
/// def factorial(n: float) -> float: ...
///
///
/// def factorial(n):
///     """Return the factorial of n."""
///
///
/// factorial.__doc__  # "Return the factorial of n."
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [Python documentation: `typing.overload`](https://docs.python.org/3/library/typing.html#typing.overload)
#[derive(ViolationMetadata)]
pub(crate) struct OverloadWithDocstring;

impl Violation for OverloadWithDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Function decorated with `@overload` shouldn't contain a docstring".to_string()
    }
}

/// D418
pub(crate) fn if_needed(checker: &Checker, docstring: &Docstring) {
    let Some(function) = docstring.definition.as_function_def() else {
        return;
    };
    if is_overload(&function.decorator_list, checker.semantic()) {
        checker.report_diagnostic(Diagnostic::new(
            OverloadWithDocstring,
            function.identifier(),
        ));
    }
}
