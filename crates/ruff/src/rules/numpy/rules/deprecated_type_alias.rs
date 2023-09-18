use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for deprecated NumPy type aliases.
///
/// ## Why is this bad?
/// NumPy's `np.int` has long been an alias of the builtin `int`. The same
/// goes for `np.float`, `np.bool`, and others. These aliases exist
/// primarily for historic reasons, and have been a cause of
/// frequent confusion for newcomers.
///
/// These aliases were been deprecated in 1.20, and removed in 1.24.
///
/// ## Examples
/// ```python
/// import numpy as np
///
/// np.bool
/// ```
///
/// Use instead:
/// ```python
/// bool
/// ```
#[violation]
pub struct NumpyDeprecatedTypeAlias {
    type_name: String,
}

impl Violation for NumpyDeprecatedTypeAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyDeprecatedTypeAlias { type_name } = self;
        format!("Type alias `np.{type_name}` is deprecated, replace with builtin type")
    }

    fn autofix_title(&self) -> Option<String> {
        let NumpyDeprecatedTypeAlias { type_name } = self;
        Some(format!("Replace `np.{type_name}` with builtin type"))
    }
}

/// NPY001
pub(crate) fn deprecated_type_alias(checker: &mut Checker, expr: &Expr) {
    if let Some(type_name) = checker
        .semantic()
        .resolve_call_path(expr)
        .and_then(|call_path| {
            if matches!(
                call_path.as_slice(),
                [
                    "numpy",
                    "bool" | "int" | "float" | "complex" | "object" | "str" | "long" | "unicode"
                ]
            ) {
                Some(call_path[1])
            } else {
                None
            }
        })
    {
        let mut diagnostic = Diagnostic::new(
            NumpyDeprecatedTypeAlias {
                type_name: type_name.to_string(),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            let type_name = match type_name {
                "unicode" => "str",
                "long" => "int",
                _ => type_name,
            };
            if checker.semantic().is_builtin(type_name) {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    type_name.to_string(),
                    expr.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
