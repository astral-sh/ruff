use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the builtin `eval()` function.
///
/// ## Why is this bad?
/// The `eval()` function is insecure as it enables arbitrary code execution.
///
/// ## Example
/// ```python
/// def foo():
///     x = eval(input("Enter a number: "))
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     x = input("Enter a number: ")
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `eval`](https://docs.python.org/3/library/functions.html#eval)
/// - [_Eval really is dangerous_ by Ned Batchelder](https://nedbatchelder.com/blog/201206/eval_really_is_dangerous.html)
#[violation]
pub struct Eval;

impl Violation for Eval {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No builtin `eval()` allowed")
    }
}

/// PGH001
pub(crate) fn no_eval(checker: &mut Checker, func: &Expr) {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return;
    };
    if id != "eval" {
        return;
    }
    if !checker.semantic().is_builtin("eval") {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(Eval, func.range()));
}
