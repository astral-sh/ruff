use ruff_python_ast::StmtImportFrom;

use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::{checkers::ast::Checker, fix};

/// ## What it does
/// Checks for the presence of the `from __future__ import annotations` import
/// statement in stub files.
///
/// ## Why is this bad?
/// Stub files natively support forward references in all contexts, as stubs are
/// never executed at runtime. (They should be thought of as "data files" for
/// type checkers.) As such, the `from __future__ import annotations` import
/// statement has no effect and should be omitted.
///
/// ## References
/// - [Static Typing with Python: Type Stubs](https://typing.readthedocs.io/en/latest/source/stubs.html)
#[violation]
pub struct FutureAnnotationsInStub;

impl Violation for FutureAnnotationsInStub {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`from __future__ import annotations` has no effect in stub files, since type checkers automatically treat stubs as having those semantics")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `from __future__ import annotations`".to_string())
    }
}

/// PYI044
pub(crate) fn from_future_import(checker: &mut Checker, target: &StmtImportFrom) {
    if let StmtImportFrom {
        range,
        module: Some(name),
        names,
        ..
    } = target
    {
        if name == "__future__" && names.iter().any(|alias| &*alias.name == "annotations") {
            let mut diagnostic = Diagnostic::new(FutureAnnotationsInStub, *range);

            if checker.settings.preview.is_enabled() {
                let stmt = checker.semantic().current_statement();

                diagnostic.try_set_fix(|| {
                    let edit = fix::edits::remove_unused_imports(
                        std::iter::once("annotations"),
                        stmt,
                        None,
                        checker.locator(),
                        checker.stylist(),
                        checker.indexer(),
                    )?;

                    Ok(Fix::safe_edit(edit))
                });
            }

            checker.diagnostics.push(diagnostic);
        }
    }
}
