use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{PySourceType, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for imports that are not at the top of the file.
///
/// ## Why is this bad?
/// According to [PEP 8], "imports are always put at the top of the file, just after any
/// module comments and docstrings, and before module globals and constants."
///
/// This rule makes an exception for both `sys.path` modifications (allowing for
/// `sys.path.insert`, `sys.path.append`, etc.) and `os.environ` modifications
/// between imports.
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
/// ## Notebook behavior
/// For Jupyter notebooks, this rule checks for imports that are not at the top of a *cell*.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[derive(ViolationMetadata)]
pub(crate) struct ModuleImportNotAtTopOfFile {
    source_type: PySourceType,
}

impl Violation for ModuleImportNotAtTopOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.source_type.is_ipynb() {
            "Module level import not at top of cell".to_string()
        } else {
            "Module level import not at top of file".to_string()
        }
    }
}

/// E402
pub(crate) fn module_import_not_at_top_of_file(checker: &Checker, stmt: &Stmt) {
    if checker.semantic().seen_import_boundary() && checker.semantic().at_top_level() {
        checker.report_diagnostic(Diagnostic::new(
            ModuleImportNotAtTopOfFile {
                source_type: checker.source_type,
            },
            stmt.range(),
        ));
    }
}
