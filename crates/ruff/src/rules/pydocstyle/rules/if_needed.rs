use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::cast;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility::is_overload;
use ruff_python_semantic::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for functions decorated with `@overload` that contain a docstring.
///
/// ## Why is this bad?
/// `@overload` is used to define multiple signatures for a function. This is
/// for type checkers only; the decorated function is overwritten by the the
/// non-decorated function. Therefore, the docstring should be defined in the
/// non-decorated function.
///
/// ## Example
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
/// ```python
/// from typing import overload
///
///
/// @overload
/// def factorial(n: int) -> int:
///     ...
///
///
/// @overload
/// def factorial(n: float) -> float:
///     ...
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
#[violation]
pub struct OverloadWithDocstring;

impl Violation for OverloadWithDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function decorated with `@overload` shouldn't contain a docstring")
    }
}

/// D418
pub(crate) fn if_needed(checker: &mut Checker, docstring: &Docstring) {
    let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = docstring.definition else {
        return;
    };
    if !is_overload(cast::decorator_list(stmt), checker.semantic()) {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(OverloadWithDocstring, stmt.identifier()));
}
