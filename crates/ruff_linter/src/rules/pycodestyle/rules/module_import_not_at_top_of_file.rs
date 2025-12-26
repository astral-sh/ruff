use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{PySourceType, Stmt};
use ruff_python_trivia::PythonWhitespace;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::{Edit, Fix, FixAvailability, Violation};

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
#[violation_metadata(stable_since = "v0.0.28")]
pub(crate) struct ModuleImportNotAtTopOfFile {
    source_type: PySourceType,
}

impl Violation for ModuleImportNotAtTopOfFile {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

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
        let mut diagnostic = checker.report_diagnostic(
            ModuleImportNotAtTopOfFile {
                source_type: checker.source_type,
            },
            stmt.range(),
        );

        let indexer = checker.indexer();
        let locator = checker.locator();

        // Special-cases: there's leading or trailing content in the import block. These
        // are too hard to get right, and relatively rare, so flag but don't fix.
        if indexer.preceded_by_multi_statement_line(stmt, locator.contents())
            || indexer.followed_by_multi_statement_line(stmt, locator.contents())
        {
            return;
        }

        let range = stmt.range();

        // Include comments but not the trailing newline (so we don't insert an extra newline).
        let text_range = TextRange::new(range.start(), locator.line_end(range.end()));

        let edit = checker.importer().add_at_start(
            checker.source()[text_range].trim_whitespace(),
            // TODO(PR): this doesn't seem to fully work -- the imports end up
            // in one of the cells above where they should, though no longer in
            // the first cell in the file.
            checker
                .cell_offsets()
                .and_then(|cell_offsets| cell_offsets.containing_range(text_range.start())),
        );

        // Include comments *and* the trailing newline, so that we do remove the whole line.
        let removal_range =
            TextRange::new(text_range.start(), locator.full_line_end(text_range.end()));

        diagnostic.set_fix(Fix::unsafe_edits(
            Edit::range_deletion(removal_range),
            vec![edit],
        ));
    }
}
