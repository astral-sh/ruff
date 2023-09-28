use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls to the Path resolve method without arguments.
///
/// ## Why is this bad?
/// Prefer using `Path.cwd()` which is explicit and avoids any confusion about the current directory.
///
/// ## Example
/// ```python
/// cwd = Path.resolve()
/// ```
///
/// Use instead:
/// ```python
/// cwd = Path.cwd()
/// ```
///
/// ## References
/// - [Python documentation: `Path.cwd`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.cwd)
///
///

#[violation]
pub struct NoImplicitCwd;

impl Violation for NoImplicitCwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid using Path.resolve() without arguments. Use Path.cwd() instead.")
    }
}

/// FURB177
pub(crate) fn no_implicit_cwd(checker: &mut Checker, call: &ExprCall) {
    let Expr::Attribute(ExprAttribute { attr: _, value, .. }) = call.func.as_ref() else {
        return;
    };

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    if !arguments.is_empty() {
        return;
    }

    if !checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["pathlib", "Path"]))
    {}
}
