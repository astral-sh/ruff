use itertools::Itertools;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_trivia::indentation_at_offset;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

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
        Some(format!("Split imports"))
    }
}

/// E401
pub(crate) fn multiple_imports_on_one_line(checker: &mut Checker, stmt: &Stmt, names: &[Alias]) {
    if names.len() > 1 {
        let mut diagnostic = Diagnostic::new(MultipleImportsOnOneLine, stmt.range());
        diagnostic.set_fix(split_imports(
            stmt,
            names,
            checker.locator(),
            checker.indexer(),
            checker.stylist(),
        ));
        checker.diagnostics.push(diagnostic);
    }
}

/// Generate a [`Fix`] to split the imports across multiple statements.
fn split_imports(
    stmt: &Stmt,
    names: &[Alias],
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Fix {
    if indexer.in_multi_statement_line(stmt, locator) {
        // Ex) `x = 1; import os, sys` (convert to `x = 1; import os; import sys`)
        let replacement = names
            .iter()
            .map(|alias| {
                let Alias {
                    range: _,
                    name,
                    asname,
                } = alias;

                if let Some(asname) = asname {
                    format!("import {name} as {asname}")
                } else {
                    format!("import {name}")
                }
            })
            .join("; ");

        Fix::safe_edit(Edit::range_replacement(replacement, stmt.range()))
    } else {
        // Ex) `import os, sys` (convert to `import os\nimport sys`)
        let indentation = indentation_at_offset(stmt.start(), locator).unwrap_or_default();

        // Generate newline-delimited imports.
        let replacement = names
            .iter()
            .map(|alias| {
                let Alias {
                    range: _,
                    name,
                    asname,
                } = alias;

                if let Some(asname) = asname {
                    format!("{indentation}import {name} as {asname}")
                } else {
                    format!("{indentation}import {name}")
                }
            })
            .join(stylist.line_ending().as_str());

        Fix::safe_edit(Edit::range_replacement(
            replacement,
            TextRange::new(locator.line_start(stmt.start()), stmt.end()),
        ))
    }
}
