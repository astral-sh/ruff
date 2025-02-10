use ruff_python_ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::ScopeKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `return` statements outside of functions.
///
/// ## Why is this bad?
/// The use of a `return` statement outside of a function will raise a
/// `SyntaxError`.
///
/// ## Example
/// ```python
/// class Foo:
///     return 1
/// ```
///
/// ## References
/// - [Python documentation: `return`](https://docs.python.org/3/reference/simple_stmts.html#the-return-statement)
#[derive(ViolationMetadata)]
pub(crate) struct ReturnOutsideFunction;

impl Violation for ReturnOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`return` statement outside of a function/method".to_string()
    }
}

pub(crate) fn return_outside_function(checker: &Checker, stmt: &Stmt) {
    if matches!(
        checker.semantic().current_scope().kind,
        ScopeKind::Class(_) | ScopeKind::Module
    ) {
        checker.report_diagnostic(Diagnostic::new(ReturnOutsideFunction, stmt.range()));
    }
}
