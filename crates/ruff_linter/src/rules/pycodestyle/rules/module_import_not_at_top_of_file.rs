use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{PySourceType, Stmt};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::preview::is_e402_fix_enabled;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
/// This rule's fix is marked as unsafe as imports moved to the top of the file
/// are placed above existing imports, in reverse order than they were in the
/// file. Re-ordering imports is unsafe as it can change the execution order of
/// the imported code.
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

    fn fix_title(&self) -> Option<String> {
        if self.source_type.is_ipynb() {
            Some("Move module level imports to top of cell".to_string())
        } else {
            Some("Move module level imports to top of file".to_string())
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

        if !is_e402_fix_enabled(checker.settings()) {
            return;
        }

        // Support for fixing notebooks is not yet implemented.
        if checker.cell_offsets().is_some() {
            return;
        }

        let indexer = checker.indexer();
        let locator = checker.locator();

        // Special-cases: there's leading or trailing content in the import block. These
        // are too hard to get right, and relatively rare, so flag but don't fix.
        if indexer.preceded_by_multi_statement_line(stmt, locator.contents())
            || indexer.followed_by_multi_statement_line(stmt, locator.contents())
        {
            return;
        }

        let edit = checker.importer().add_import_at_start(stmt);

        // Include trailing comments and the newline in the removal.
        let removal_range = TextRange::new(stmt.start(), locator.full_line_end(stmt.end()));

        diagnostic.set_fix(Fix::unsafe_edits(
            Edit::range_deletion(removal_range),
            [edit],
        ));
    }
}
