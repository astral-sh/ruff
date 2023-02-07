use crate::define_violation;
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;

define_violation!(
    pub struct OpenAlias;
);
impl AlwaysAutofixableViolation for OpenAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use builtin `open`")
    }

    fn autofix_title(&self) -> String {
        "Replace with builtin `open`".to_string()
    }
}

/// UP020
pub fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if checker
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["io", "open"])
    {
        let mut diagnostic = Diagnostic::new(OpenAlias, Range::from_located(expr));
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                "open".to_string(),
                func.location,
                func.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
