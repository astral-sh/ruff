use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of deprecated NumPy functions.
///
/// ## Why is this bad?
/// When NumPy functions are deprecated, they are usually replaced with
/// newer, more efficient versions, or with functions that are more
/// consistent with the rest of the NumPy API.
///
/// Prefer newer APIs over deprecated ones.
///
/// ## Examples
/// ```python
/// import numpy as np
///
/// np.alltrue([True, False])
/// ```
///
/// Use instead:
/// ```python
/// import numpy as np
///
/// np.all([True, False])
/// ```
#[violation]
pub struct NumpyDeprecatedFunction {
    existing: String,
    replacement: String,
}

impl Violation for NumpyDeprecatedFunction {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyDeprecatedFunction {
            existing,
            replacement,
        } = self;
        format!("`np.{existing}` is deprecated; use `np.{replacement}` instead")
    }

    fn autofix_title(&self) -> Option<String> {
        let NumpyDeprecatedFunction { replacement, .. } = self;
        Some(format!("Replace with `np.{replacement}`"))
    }
}

/// NPY003
pub(crate) fn deprecated_function(checker: &mut Checker, expr: &Expr) {
    if let Some((existing, replacement)) =
        checker
            .semantic()
            .resolve_call_path(expr)
            .and_then(|call_path| match call_path.as_slice() {
                ["numpy", "round_"] => Some(("round_", "round")),
                ["numpy", "product"] => Some(("product", "prod")),
                ["numpy", "cumproduct"] => Some(("cumproduct", "cumprod")),
                ["numpy", "sometrue"] => Some(("sometrue", "any")),
                ["numpy", "alltrue"] => Some(("alltrue", "all")),
                _ => None,
            })
    {
        let mut diagnostic = Diagnostic::new(
            NumpyDeprecatedFunction {
                existing: existing.to_string(),
                replacement: replacement.to_string(),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_symbol(
                    &ImportRequest::import_from("numpy", replacement),
                    expr.start(),
                    checker.semantic(),
                )?;
                let replacement_edit = Edit::range_replacement(binding, expr.range());
                Ok(Fix::suggested_edits(import_edit, [replacement_edit]))
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
