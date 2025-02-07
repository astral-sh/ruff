use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::ScopeKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `yield from` in async functions.
///
/// ## Why is this bad?
/// Python doesn't support the use of `yield from` in async functions, and will
/// raise a `SyntaxError` in such cases.
///
/// Instead, considering refactoring the code to use an `async for` loop instead.
///
/// ## Example
/// ```python
/// async def numbers():
///     yield from [1, 2, 3, 4, 5]
/// ```
///
/// Use instead:
/// ```python
/// async def numbers():
///     async for number in [1, 2, 3, 4, 5]:
///         yield number
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct YieldFromInAsyncFunction;

impl Violation for YieldFromInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`yield from` statement in async function; use `async for` instead".to_string()
    }
}

/// PLE1700
pub(crate) fn yield_from_in_async_function(checker: &Checker, expr: &ast::ExprYieldFrom) {
    if matches!(
        checker.semantic().current_scope().kind,
        ScopeKind::Function(ast::StmtFunctionDef { is_async: true, .. })
    ) {
        checker.report_diagnostic(Diagnostic::new(YieldFromInAsyncFunction, expr.range()));
    }
}
