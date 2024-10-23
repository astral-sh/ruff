use ruff_python_ast::ExprStringLiteral;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of docstrings in stub files.
///
/// ## Why is this bad?
/// Stub files should omit docstrings, as they're intended to provide type
/// hints, rather than documentation.
///
/// ## Example
///
/// ```pyi
/// def func(param: int) -> str:
///     """This is a docstring."""
///     ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def func(param: int) -> str: ...
/// ```
#[violation]
pub struct DocstringInStub;

impl Violation for DocstringInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstrings should not be included in stubs")
    }
}

/// PYI021
pub(crate) fn docstring_in_stubs(checker: &mut Checker, docstring: Option<&ExprStringLiteral>) {
    if let Some(docstr) = docstring {
        checker
            .diagnostics
            .push(Diagnostic::new(DocstringInStub, docstr.range()));
    }
}
