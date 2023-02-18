use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

use super::yield_in_init::in_dunder_init;

define_violation!(
    /// ## What it does
    /// Checks for `__init__` methods that return values.
    ///
    /// ## Why is this bad?
    /// The `__init__` method is the constructor for a given Python class,
    /// responsible for initializing, rather than creating, new objects.
    ///
    /// The `__init__` method has to return `None`. If it returns `self` or any
    /// other objects, this results in a runtime error.
    ///
    /// ## Example
    /// ```python
    /// class InitReturnsValue:
    ///     def __init__(self, i):
    ///         return []
    /// ```
    ///
    /// ## References
    /// * [CodeQL: `py-explicit-return-in-init`](https://codeql.github.com/codeql-query-help/python/py-explicit-return-in-init/)
    pub struct ReturnInInit;
);
impl Violation for ReturnInInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Explicit return in `__init__`")
    }
}

/// PLE0101
pub fn return_in_init(checker: &mut Checker, stmt: &Stmt) {
    if let StmtKind::Return { value } = &stmt.node {
        if let Some(expr) = value {
            if matches!(
                expr.node,
                ExprKind::Constant {
                    value: Constant::None,
                    ..
                }
            ) {
                // return None
                return;
            }
        } else {
            // return with no value
            return;
        }
    }

    if in_dunder_init(checker) {
        checker
            .diagnostics
            .push(Diagnostic::new(ReturnInInit, Range::from_located(stmt)));
    }
}
