use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::in_dunder_method;

/// ## What it does
/// Checks for `__init__` methods that return values.
///
/// ## Why is this bad?
/// The `__init__` method is the constructor for a given Python class,
/// responsible for initializing, rather than creating, new objects.
///
/// The `__init__` method has to return `None`. Returning any value from
/// an `__init__` method will result in a runtime error.
///
/// ## Example
/// ```python
/// class Example:
///     def __init__(self):
///         return []
/// ```
///
/// Use instead:
/// ```python
/// class Example:
///     def __init__(self):
///         self.value = []
/// ```
///
/// ## References
/// - [CodeQL: `py-explicit-return-in-init`](https://codeql.github.com/codeql-query-help/python/py-explicit-return-in-init/)
#[derive(ViolationMetadata)]
pub(crate) struct ReturnInInit;

impl Violation for ReturnInInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Explicit return in `__init__`".to_string()
    }
}

/// PLE0101
pub(crate) fn return_in_init(checker: &Checker, stmt: &Stmt) {
    if let Stmt::Return(ast::StmtReturn { value, range: _ }) = stmt {
        if let Some(expr) = value {
            if expr.is_none_literal_expr() {
                // Explicit `return None`.
                return;
            }
        } else {
            // Implicit `return`.
            return;
        }
    }

    if in_dunder_method("__init__", checker.semantic(), checker.settings) {
        checker.report_diagnostic(Diagnostic::new(ReturnInInit, stmt.range()));
    }
}
