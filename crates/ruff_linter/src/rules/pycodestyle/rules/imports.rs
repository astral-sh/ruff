use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, PySourceType, Stmt};
use ruff_python_trivia::{indentation_at_offset, PythonWhitespace};
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple imports on one line")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Split imports onto multiple lines"))
    }
}

/// ## What it does
/// Checks for imports that are not at the top of the file. For Jupyter notebooks, this
/// checks for imports that are not at the top of the cell.
///
/// ## Why is this bad?
/// According to [PEP 8], "imports are always put at the top of the file, just after any
/// module comments and docstrings, and before module globals and constants."
///
/// In [preview], this rule makes an exception for `sys.path` modifications,
/// allowing for `sys.path.insert`, `sys.path.append`, and similar
/// modifications between import statements.
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
/// [preview]: https://docs.astral.sh/ruff/preview/
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

/// E401
pub(crate) fn multiple_imports_on_one_line(checker: &mut Checker, stmt: &Stmt, names: &[Alias]) {
    if names.len() > 1 {
        let mut diagnostic = Diagnostic::new(MultipleImportsOnOneLine, stmt.range());

        if checker.settings.preview.is_enabled() {
            let indentation = indentation_at_offset(stmt.start(), checker.locator()).unwrap_or("");

            let mut replacement = String::new();

            for item in names {
                let Alias {
                    range: _,
                    name,
                    asname,
                } = item;

                if let Some(asname) = asname {
                    replacement = format!("{replacement}{indentation}import {name} as {asname}\n");
                } else {
                    replacement = format!("{replacement}{indentation}import {name}\n");
                }
            }

            // remove leading whitespace because we start at the import keyword
            replacement = replacement.trim_whitespace_start().to_string();

            // remove trailing newline
            replacement = replacement.trim_end_matches('\n').to_string();

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                replacement,
                stmt.range(),
            )));
        }

        checker.diagnostics.push(diagnostic);
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
