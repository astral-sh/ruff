use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::{
    ast::types::{FunctionDef, Range, ScopeKind},
    checkers::ast::Checker,
    registry::Diagnostic,
    violation::Violation,
};

define_violation!(
    pub struct YieldInInit;
);

impl Violation for YieldInInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__init__` method is a generator")
    }
}

/// PLE0100
pub fn yield_in_init(checker: &mut Checker, expr: &Expr) {
    let parent_scope_is_class: Option<bool> =
        checker
            .current_scopes()
            .nth(1)
            .and_then(|scope| match scope.kind {
                ScopeKind::Class(..) => Some(true),
                _ => None,
            });

    let current_scope_is_init = match checker.current_scope().kind {
        ScopeKind::Function(FunctionDef { name, .. }) => Some(name == "__init__"),
        _ => None,
    };

    if parent_scope_is_class == Some(true) && current_scope_is_init == Some(true) {
        checker
            .diagnostics
            .push(Diagnostic::new(YieldInInit, Range::from_located(expr)));
    }
}
