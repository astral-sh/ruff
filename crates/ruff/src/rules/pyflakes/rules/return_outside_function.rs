use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

#[violation]
pub struct ReturnOutsideFunction;

impl Violation for ReturnOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`return` statement outside of a function/method")
    }
}

pub fn return_outside_function(checker: &mut Checker, stmt: &Stmt) {
    if let Some(index) = checker.ctx.scope_stack.top() {
        if matches!(
            checker.ctx.scopes[index].kind,
            ScopeKind::Class(_) | ScopeKind::Module
        ) {
            checker
                .diagnostics
                .push(Diagnostic::new(ReturnOutsideFunction, Range::from(stmt)));
        }
    }
}
