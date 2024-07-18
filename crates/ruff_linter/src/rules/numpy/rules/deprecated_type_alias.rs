use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for deprecated NumPy type aliases.
///
/// ## Why is this bad?
/// NumPy's `np.int` has long been an alias of the builtin `int`; the same
/// is true of `np.float` and others. These aliases exist primarily
/// for historic reasons, and have been a cause of frequent confusion
/// for newcomers.
///
/// These aliases were deprecated in 1.20, and removed in 1.24.
/// Note, however, that `np.bool` and `np.long` were reintroduced in 2.0 with
/// different semantics, and are thus omitted from this rule.
///
/// ## Examples
/// ```python
/// import numpy as np
///
/// np.int
/// ```
///
/// Use instead:
/// ```python
/// int
/// ```
#[violation]
pub struct NumpyDeprecatedTypeAlias {
    type_name: String,
}

impl Violation for NumpyDeprecatedTypeAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyDeprecatedTypeAlias { type_name } = self;
        format!("Type alias `np.{type_name}` is deprecated, replace with builtin type")
    }

    fn fix_title(&self) -> Option<String> {
        let NumpyDeprecatedTypeAlias { type_name } = self;
        Some(format!("Replace `np.{type_name}` with builtin type"))
    }
}

/// NPY001
pub(crate) fn deprecated_type_alias(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::NUMPY) {
        return;
    }

    if let Some(type_name) =
        checker
            .semantic()
            .resolve_qualified_name(expr)
            .and_then(|qualified_name| {
                if matches!(
                    qualified_name.segments(),
                    [
                        "numpy",
                        "int" | "float" | "complex" | "object" | "str" | "unicode"
                    ]
                ) {
                    Some(qualified_name.segments()[1])
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
        let type_name = match type_name {
            "unicode" => "str",
            _ => type_name,
        };
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                type_name,
                expr.start(),
                checker.semantic(),
            )?;
            let binding_edit = Edit::range_replacement(binding, expr.range());
            Ok(Fix::safe_edits(binding_edit, import_edit))
        });
        checker.diagnostics.push(diagnostic);
    }
}
