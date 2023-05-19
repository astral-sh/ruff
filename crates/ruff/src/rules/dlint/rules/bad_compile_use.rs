use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that code does not call the built-in function `compile`
///
/// ## Why is this bad?
/// While not bad in and of itself, this function is _probably_ a code smell that
/// something else we don't want could be going on. I.e. "compile" is often proceeded by
/// "eval" or "exec".
///
/// ## Example
/// ```python
/// compile("foo")
/// ```
#[violation]
pub struct BadCompileUse;

impl Violation for BadCompileUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `compile` command should be avoided")
    }
}

/// DUO110
pub(crate) fn bad_compile_use(checker: &mut Checker, expr: &Expr) {
    if let Expr::Call(ast::ExprCall { func, .. }) = expr {
        if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
            if id == "compile" && checker.ctx.is_builtin(id) {
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadCompileUse, func.range()));
                }
            }
        }
    }
}
