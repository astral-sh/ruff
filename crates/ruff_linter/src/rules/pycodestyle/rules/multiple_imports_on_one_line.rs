use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for multiple imports on one line.
///
/// ## Why is this bad?
/// According to [PEP 8], "imports should usually be on separate lines."
///
/// ## Example
/// ```python
/// import sys, os
/// ```
///
/// Use instead:
/// ```python
/// import os
/// import sys
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[violation]
pub struct MultipleImportsOnOneLine;

impl Violation for MultipleImportsOnOneLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple imports on one line")
    }
}

/// E401
pub(crate) fn multiple_imports_on_one_line(checker: &mut Checker, stmt: &Stmt, names: &[Alias]) {
    if names.len() > 1 {
        checker
            .diagnostics
            .push(Diagnostic::new(MultipleImportsOnOneLine, stmt.range()));
    }
}
