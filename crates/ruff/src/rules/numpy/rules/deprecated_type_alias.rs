use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for deprecated NumPy type aliases.
///
/// ## Why is this bad?
/// NumPy's `np.int` has long been an alias of the builtin `int`. The same
/// goes for `np.float`, `np.bool`, and others. These aliases exist
/// primarily primarily for historic reasons, and have been a cause of
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

impl AlwaysAutofixableViolation for NumpyDeprecatedTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyDeprecatedTypeAlias { type_name } = self;
        format!("Type alias `np.{type_name}` is deprecated, replace with builtin type")
    }

    fn autofix_title(&self) -> String {
        let NumpyDeprecatedTypeAlias { type_name } = self;
        format!("Replace `np.{type_name}` with builtin type")
    }
}

/// NPY001
pub(crate) fn deprecated_type_alias(checker: &mut Checker, expr: &Expr) {
    if let Some(type_name) =
        checker
            .semantic_model()
            .resolve_call_path(expr)
            .and_then(|call_path| {
                if call_path.as_slice() == ["numpy", "bool"]
                    || call_path.as_slice() == ["numpy", "int"]
                    || call_path.as_slice() == ["numpy", "float"]
                    || call_path.as_slice() == ["numpy", "complex"]
                    || call_path.as_slice() == ["numpy", "object"]
                    || call_path.as_slice() == ["numpy", "str"]
                    || call_path.as_slice() == ["numpy", "long"]
                    || call_path.as_slice() == ["numpy", "unicode"]
                {
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
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                match type_name {
                    "unicode" => "str",
                    "long" => "int",
                    _ => type_name,
                }
                .to_string(),
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
