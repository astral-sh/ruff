use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of `logging.WARN`.
///
/// ## Why is this bad?
/// The `logging.WARN` constant is an undocumented alias for `logging.WARNING`.
///
/// Although it’s not explicitly deprecated, `logging.WARN` is not mentioned
/// in the `logging` documentation. Prefer `logging.WARNING` instead.
///
/// ## Example
/// ```python
/// import logging
///
///
/// logging.basicConfig(level=logging.WARN)
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
///
/// logging.basicConfig(level=logging.WARNING)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UndocumentedWarn;

impl Violation for UndocumentedWarn {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of undocumented `logging.WARN` constant".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace `logging.WARN` with `logging.WARNING`".to_string())
    }
}

/// LOG009
pub(crate) fn undocumented_warn(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::LOGGING) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["logging", "WARN"]))
    {
        let mut diagnostic = Diagnostic::new(UndocumentedWarn, expr.range());
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("logging", "WARNING"),
                expr.start(),
                checker.semantic(),
            )?;
            let reference_edit = Edit::range_replacement(binding, expr.range());
            Ok(Fix::safe_edits(import_edit, [reference_edit]))
        });
        checker.report_diagnostic(diagnostic);
    }
}
