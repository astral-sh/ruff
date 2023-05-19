use rustpython_parser::ast::{self, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usages of a named expression used to do a regular assignment
/// outside a context like `if`, `for`, `while`, or a comprehension.
///
/// ## Why is this bad?
/// Whilel technically correct code, it adds unnecessary complexity.
///
/// ## Example
/// ```python
/// (a := 42)
/// ```
///
/// Use instead:
/// ```python
/// a = 42
/// ```
#[violation]
pub struct NamedExprWithoutContext;

impl Violation for NamedExprWithoutContext {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Named expression used without context")
    }
}

/// PLW0131
pub(crate) fn named_expr_without_context(checker: &mut Checker, value: &Expr) {
    if let Expr::NamedExpr(ast::ExprNamedExpr { range, .. }) = value {
        checker
            .diagnostics
            .push(Diagnostic::new(NamedExprWithoutContext {}, *range));
    }
}
