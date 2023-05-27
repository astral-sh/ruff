use rustpython_parser::ast::{self, Constant, Expr, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct AnyEqNeNotation {
    method: String,
}

impl AlwaysAutofixableViolation for AnyEqNeNotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `object` to `Any` for the second parameter in {method}")
    }

    fn autofix_title(&self) -> String {
        format!("Replace `object` with `Any`")
    }
}

/// PYI032
pub(crate) fn any_eq_ne_annotation(checker: &mut Checker, body: &[Stmt]) {
    // TODO: Implement
}
