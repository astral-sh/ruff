use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `return` statements outside of a function.
///
/// ## Why is this bad?
/// `return` statements outside a function will raise `SyntaxError`.
///
/// ## Example
/// ```python
/// class Foo:
///     return 1  # SyntaxError
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-return-statement)
#[violation]
pub struct ReturnOutsideFunction;

impl Violation for ReturnOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`return` statement outside of a function/method")
    }
}

pub(crate) fn return_outside_function(checker: &mut Checker, stmt: &Stmt) {
    if matches!(
        checker.semantic_model().scope().kind,
        ScopeKind::Class(_) | ScopeKind::Module
    ) {
        checker
            .diagnostics
            .push(Diagnostic::new(ReturnOutsideFunction, stmt.range()));
    }
}
