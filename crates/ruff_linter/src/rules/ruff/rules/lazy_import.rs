use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{PythonVersion, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{Edit, Fix, FixAvailability, Violation};

/// Checks for local imports that could be `lazy` imports.
///
/// ## Why is this bad?
/// An import inside a function body is often a way to defer a costly import
/// until it's needed. PEP 810 lazy imports (Python 3.15+) express that intent
/// declaratively at the top level of the module, letting the interpreter defer
/// the load itself rather than paying it at every call site.
///
/// ## Example
///
/// ```python
/// def analyze(data):
///     import pandas
///
///     return pandas.DataFrame(data)
/// ```
///
/// Use instead:
/// ```python
/// lazy import pandas
///
///
/// def analyze(data):
///     return pandas.DataFrame(data)
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [PEP 810 — Explicit lazy imports](https://peps.python.org/pep-0810/)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.17.4", category = Category::Performance)]
pub(crate) struct LazyImport;

impl Violation for LazyImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`import` inside a function can be a `lazy` import".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add the `lazy` keyword".to_string())
    }
}

/// RUF078
pub(crate) fn lazy_import(checker: &Checker, stmt: &Stmt) {
    // `lazy` imports only parse on Python 3.15+.
    if checker.target_version() < PythonVersion::PY315 {
        return;
    }

    let is_lazy = match stmt {
        Stmt::Import(stmt_import) => stmt_import.is_lazy,
        Stmt::ImportFrom(stmt_import_from) => stmt_import_from.is_lazy,
        _ => return,
    };

    if is_lazy || checker.semantic().current_scope().kind.is_module() {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(LazyImport, stmt.range());
    // Deferring an import changes when its side effects run.
    diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
        "lazy ".to_string(),
        stmt.range().start(),
    )));
}
