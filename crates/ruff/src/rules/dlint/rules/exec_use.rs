use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that code does not call the built-in function `exec`
///
/// ## Why is this bad?
/// `exec` supports dynamic execution of Python code by passing either a string or a code object
/// to it, which are then parsed and executed or simply executed respectively. If a string or code
/// object is passed from an untrusted source  (or contains parts from an untrusted source) it can
/// be used to run malicious code
///
/// ## Example
/// ```python
/// exec("foo")
/// ```
#[violation]
pub struct ExecUse;

impl Violation for ExecUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `exec` command should be avoided")
    }
}

/// DUO105
pub(crate) fn exec_use(checker: &mut Checker, expr: &ast::ExprCall) {
    if let Expr::Name(ast::ExprName { id, .. }) = expr.func.as_ref() {
        if id == "exec" && checker.model.is_builtin(id) {
            {
                checker
                    .diagnostics
                    .push(Diagnostic::new(ExecUse, expr.func.range()));
            }
        }
    }
}
