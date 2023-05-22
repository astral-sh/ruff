use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that code does not call the built-in function `eval`
///
/// ## Why is this bad?
/// The `eval` function can be used to execute arbitrary code objects if a code object is passed
/// as the expression argument. If a code object is passed from an untrusted source it can be used
/// to run malicious code
///
/// ## Example
/// ```python
/// eval("foo")
/// ```
#[violation]
pub struct EvalUse;

impl Violation for EvalUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `eval` command should be avoided")
    }
}

/// DUO104
pub(crate) fn bad_eval_use(checker: &mut Checker, expr: &ast::ExprCall) {
        if let Expr::Name(ast::ExprName { id, .. }) = expr.func.as_ref() {
            if id == "eval" && checker.ctx.is_builtin(id) {
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(EvalUse, expr.func.range()));
                }
            }
        }
    }
