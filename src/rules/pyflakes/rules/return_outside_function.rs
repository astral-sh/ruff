use crate::ast::types::{Range, ScopeKind};
use crate::checkers::ast::Checker;
use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Stmt;

define_simple_violation!(
    ReturnOutsideFunction,
    "`return` statement outside of a function/method"
);

pub fn return_outside_function(checker: &mut Checker, stmt: &Stmt) {
    if let Some(&index) = checker.scope_stack.last() {
        if matches!(
            checker.scopes[index].kind,
            ScopeKind::Class(_) | ScopeKind::Module
        ) {
            checker.diagnostics.push(Diagnostic::new(
                ReturnOutsideFunction,
                Range::from_located(stmt),
            ));
        }
    }
}
