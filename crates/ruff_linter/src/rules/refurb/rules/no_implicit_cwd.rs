use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for current-directory lookups using `Path().resolve()`.
///
/// ## Why is this bad?
/// When looking up the current directory, prefer `Path.cwd()` over
/// `Path().resolve()`, as `Path.cwd()` is more explicit in its intent.
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

#[violation]
pub struct NoImplicitCwd;

impl Violation for NoImplicitCwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `Path.cwd()` over `Path().resolve()` for current-directory lookups")
    }
}

/// FURB177
pub(crate) fn no_implicit_cwd(checker: &mut Checker, call: &ExprCall) {
    if !call.arguments.is_empty() {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr != "resolve" {
        return;
    }

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
    {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(NoImplicitCwd, call.range()))
}
