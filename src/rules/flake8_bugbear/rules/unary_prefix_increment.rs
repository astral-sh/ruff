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

use rustpython_ast::{Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// B002
pub fn unary_prefix_increment(checker: &mut Checker, expr: &Expr, op: &Unaryop, operand: &Expr) {
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
        violations::UnaryPrefixIncrement,
        Range::from_located(expr),
    ));
}
