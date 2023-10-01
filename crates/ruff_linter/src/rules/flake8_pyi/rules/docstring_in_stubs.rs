use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of docstrings in stub files.
///
/// ## Why is this bad?
/// In stub files, docstrings should not be included, as they are meant to
/// provide type hints and not documentation.
///
/// ## Example
/// ```python
/// def my_function(param: int) -> str:
///     """This is a docstring."""
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def my_function(param: int) -> str:
///     ...
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
pub(crate) fn docstring_in_stubs(checker: &mut Checker, docstring: Option<&Expr>) {
    if let Some(docstr) = &docstring {
        checker
            .diagnostics
            .push(Diagnostic::new(DocstringInStub, docstr.range()));
    }
}
