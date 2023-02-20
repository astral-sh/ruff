//! Checks for `++n`.
//!
//! ## Why is this bad?
//!
//! Python does not support the unary prefix increment. Writing `++n` is
//! equivalent to `+(+(n))`, which equals `n`.
//!
//! ## Example
//!
//! ```python
//! ++n;
//! ```
//!
//! Use instead:
//!
//! ```python
//! n += 1
//! ```

use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct ExceptWithEmptyTuple;
);
impl Violation for ExceptWithEmptyTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using except (): with an empty tuple does not handle/catch anything. Add exceptions to handle.")
    }
}

/// B029
pub fn except_with_empty_tuple(checker: &mut Checker, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::UAdd) {
        return;
    }
    let ExprKind::UnaryOp { op, .. } = &operand.node else {
            return;
        };
    if !matches!(op, Unaryop::UAdd) {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        ExceptWithEmptyTuple,
        Range::from_located(expr),
    ));
}
