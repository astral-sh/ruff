use ruff_python_ast::Expr;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `WARN`
///
/// ## Why is this bad?
/// The WARN constant is an undocumented alias for WARNING. Whilst it’s not deprecated, it’s not
/// mentioned at all in the documentation, so the documented WARNING should always be used instead.
///
/// ## Example
/// ```python
/// logging.basicConfig(level=logging.WARN)
/// ```
///
/// Use instead:
/// ```python
/// logging.basicConfig(level=logging.WARNING)
/// ```
#[violation]
pub struct UndocumentedWarn;

impl AlwaysAutofixableViolation for UndocumentedWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of undocumented logging.WARN constant")
    }

    fn autofix_title(&self) -> String {
        format!("Replace logging.WARN with logging.WARNING")
    }
}

/// LOG009
pub(crate) fn undocumented_warn(checker: &mut Checker, expr: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(expr)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["logging", "WARN"]))
    {
        let mut diagnostic = Diagnostic::new(UndocumentedWarn, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_symbol(
                    &ImportRequest::import("logging", "WARNING"),
                    expr.range().start(),
                    checker.semantic(),
                )?;
                let reference_edit = Edit::range_replacement(binding, expr.range());
                Ok(Fix::suggested_edits(import_edit, [reference_edit]))
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
