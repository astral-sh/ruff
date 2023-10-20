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

/// ## What it does
/// Checks for imports that are not at the top of the file.
///
/// ## Why is this bad?
/// According to [PEP 8], "imports are always put at the top of the file, just after any
/// module comments and docstrings, and before module globals and constants."
///
/// ## Example
/// ```python
/// "One string"
/// "Two string"
/// a = 1
/// import os
/// from sys import x
/// ```
///
/// Use instead:
/// ```python
/// import os
/// from sys import x
///
/// "One string"
/// "Two string"
/// a = 1
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[violation]
pub struct ModuleImportNotAtTopOfFile;

impl Violation for ModuleImportNotAtTopOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Module level import not at top of file")
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

/// E402
pub(crate) fn module_import_not_at_top_of_file(checker: &mut Checker, stmt: &Stmt) {
    if checker.semantic().seen_import_boundary() && checker.semantic().at_top_level() {
        checker
            .diagnostics
            .push(Diagnostic::new(ModuleImportNotAtTopOfFile, stmt.range()));
    }
}
