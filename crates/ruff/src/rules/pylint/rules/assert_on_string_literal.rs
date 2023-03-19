use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `assert` statements that use a string literal as the first
/// argument.
///
/// ## Why is this bad?
/// An `assert` on a string literal will always pass.
///
/// ## Example
/// ```python
/// assert "always true"
/// ```
///
/// Use instead:
/// ```python
/// assert a == 3
/// ```
#[violation]
pub struct AssertOnStringLiteral;

impl Violation for AssertOnStringLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Asserting on a string literal will always pass")
    }
}

/// PLW0129
pub fn assert_on_string_literal(checker: &mut Checker, test: &Expr) {
    if matches!(
        test.node,
        ExprKind::Constant {
            value: Constant::Str(..),
            ..
        }
    ) {
        checker
            .diagnostics
            .push(Diagnostic::new(AssertOnStringLiteral, Range::from(test)));
    }
}
