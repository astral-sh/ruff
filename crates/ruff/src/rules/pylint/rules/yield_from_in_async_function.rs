use rustpython_parser::ast::{Expr, Ranged, StmtAsyncFunctionDef};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks if `yield from` is used inside of an async function
///
/// ## Why is this bad?
/// This will result in a `SyntaxError`
///
/// ## Example
/// ```python
/// def foo():
///     l = (1, 2, 3)
///     yield from l
/// ```
///
/// ## References
/// [pylint E1700: `yield-inside-async-function`] https://pylint.pycqa.org/en/latest/user_guide/messages/error/yield-inside-async-function.html
#[violation]
pub struct YieldFromInAsyncFunction;

impl Violation for YieldFromInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`yield from` inside async function consider refactoring with `async for`")
    }
}

/// PLE1700
pub(crate) fn yield_from_in_async_function(checker: &mut Checker, expr: &Expr) {
    let scope = checker.semantic_model().scope();
    if let ScopeKind::AsyncFunction(StmtAsyncFunctionDef { .. }) = scope.kind {
        checker
            .diagnostics
            .push(Diagnostic::new(YieldFromInAsyncFunction, expr.range()));
    }
}
