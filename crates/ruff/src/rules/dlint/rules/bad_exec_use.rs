use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that code does not call the built-in function `exec`
///
/// ## Why is this bad?
/// This function makes it far too easy to achieve arbitrary code execution, so we shouldn't
/// support it in any context.
///
/// ## Example
/// ```python
/// exec("foo")
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct BadExecUse;

impl Violation for BadExecUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `exec` command should be avoided")
    }
}

/// DUO105
pub(crate) fn bad_exec_use(checker: &mut Checker, expr: &Expr) {
    if let Expr::Call(ast::ExprCall { func, .. }) = expr {
        if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
            if id == "exec" && checker.ctx.is_builtin(id) {
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadExecUse, func.range()));
                }
            }
        }
    }
}
