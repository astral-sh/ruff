use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{PySourceType, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for imports that are not at the top of the file. For Jupyter notebooks, this
/// checks for imports that are not at the top of the cell.
///
/// ## Why is this bad?
/// According to [PEP 8], "imports are always put at the top of the file, just after any
/// module comments and docstrings, and before module globals and constants."
///
/// This rule makes an exception for `sys.path` modifications,  allowing for
/// `sys.path.insert`, `sys.path.append`, and similar modifications between import
/// statements.
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
pub struct ModuleImportNotAtTopOfFile {
    source_type: PySourceType,
}

impl Violation for ModuleImportNotAtTopOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.source_type.is_ipynb() {
            format!("Module level import not at top of cell")
        } else {
            format!("Module level import not at top of file")
        }
    }
}

/// E402
pub(crate) fn module_import_not_at_top_of_file(checker: &mut Checker, stmt: &Stmt) {
    if checker.semantic().seen_import_boundary() && checker.semantic().at_top_level() {
        checker.diagnostics.push(Diagnostic::new(
            ModuleImportNotAtTopOfFile {
                source_type: checker.source_type,
            },
            stmt.range(),
        ));
    }
}
