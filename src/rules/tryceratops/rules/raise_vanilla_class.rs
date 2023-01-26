use ruff_macros::derive_message_formats;
use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct RaiseVanillaClass;
);
impl Violation for RaiseVanillaClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Create your own exception")
    }
}

/// TRY002
pub fn raise_vanilla_class(checker: &mut Checker, expr: &Expr) {
    if checker
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == ["", "Exception"])
    {
        checker.diagnostics.push(Diagnostic::new(
            RaiseVanillaClass,
            Range::from_located(expr),
        ));
    }
}
