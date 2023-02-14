use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Stmt;

use crate::ast::types::{Range, ScopeKind};
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct ReturnOutsideFunction;
);
impl Violation for ReturnOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`return` statement outside of a function/method")
    }
}

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
