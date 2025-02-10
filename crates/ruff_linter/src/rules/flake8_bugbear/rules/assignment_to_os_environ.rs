use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignments to `os.environ`.
///
/// ## Why is this bad?
/// In Python, `os.environ` is a mapping that represents the environment of the
/// current process.
///
/// However, reassigning to `os.environ` does not clear the environment. Instead,
/// it merely updates the `os.environ` for the current process. This can lead to
/// unexpected behavior, especially when running the program in a subprocess.
///
/// Instead, use `os.environ.clear()` to clear the environment, or use the
/// `env` argument of `subprocess.Popen` to pass a custom environment to
/// a subprocess.
///
/// ## Example
/// ```python
/// import os
///
/// os.environ = {"foo": "bar"}
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// os.environ.clear()
/// os.environ["foo"] = "bar"
/// ```
///
/// ## References
/// - [Python documentation: `os.environ`](https://docs.python.org/3/library/os.html#os.environ)
/// - [Python documentation: `subprocess.Popen`](https://docs.python.org/3/library/subprocess.html#subprocess.Popen)
#[derive(ViolationMetadata)]
pub(crate) struct AssignmentToOsEnviron;

impl Violation for AssignmentToOsEnviron {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Assigning to `os.environ` doesn't clear the environment".to_string()
    }
}

/// B003
pub(crate) fn assignment_to_os_environ(checker: &Checker, targets: &[Expr]) {
    let [target] = targets else {
        return;
    };
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = target else {
        return;
    };
    if attr != "environ" {
        return;
    }
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return;
    };
    if id != "os" {
        return;
    }
    checker.report_diagnostic(Diagnostic::new(AssignmentToOsEnviron, target.range()));
}
