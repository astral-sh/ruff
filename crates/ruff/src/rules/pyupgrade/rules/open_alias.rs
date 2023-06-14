use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct OpenAlias;

impl Violation for OpenAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use builtin `open`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with builtin `open`".to_string())
    }
}

/// UP020
pub(crate) fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if checker
        .semantic_model()
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["io", "open"])
    {
        let mut diagnostic = Diagnostic::new(OpenAlias, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            if checker.semantic_model().is_available("open") {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    "open".to_string(),
                    func.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
