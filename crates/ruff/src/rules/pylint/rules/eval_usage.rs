use crate::violation::Violation;
use crate::{ast::types::Range, checkers::ast::Checker, registry::Diagnostic};
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

define_violation!(
    /// ## What it does
    /// Checks for use of `eval`.
    ///
    /// ## Why is this bad?
    /// Pylint discourages the use of `eval` because there is a
    /// safer alternative with `ast.literal_eval`.
    ///
    /// ## Example
    /// ```python
    /// eval("print('test')")
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from ast import literal_eval
    /// literal_eval("print('test')")
    /// ```
    pub struct EvalUsage;
);
impl Violation for EvalUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't use eval")
    }
}

pub fn eval_usage(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Name { id, ctx: _ } = &expr.node {
        if id == "eval" {
            checker
                .diagnostics
                .push(Diagnostic::new(EvalUsage, Range::from_located(expr)));
        }
    }
}
