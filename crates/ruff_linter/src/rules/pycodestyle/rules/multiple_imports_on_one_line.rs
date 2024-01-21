use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, Stmt};
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
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple imports on one line")
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
