use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;

#[violation]
pub struct AwaitOutsideAsync;

impl Violation for AwaitOutsideAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`await` should be used within an async function")
    }
}

/// PLE1142
pub fn await_outside_async(checker: &mut Checker, expr: &Expr) {
    if !checker
        .ctx
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(true)
    {
        checker
            .diagnostics
            .push(Diagnostic::new(AwaitOutsideAsync, Range::from(expr)));
    }
}
