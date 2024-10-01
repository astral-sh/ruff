use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the builtin `exec` function.
///
/// ## Why is this bad?
/// The `exec()` function is insecure as it allows for arbitrary code
/// execution.
///
/// ## Example
/// ```python
/// exec("print('Hello World')")
/// ```
///
/// ## References
/// - [Python documentation: `exec`](https://docs.python.org/3/library/functions.html#exec)
/// - [Common Weakness Enumeration: CWE-78](https://cwe.mitre.org/data/definitions/78.html)
#[violation]
pub struct ExecBuiltin;

impl Violation for ExecBuiltin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `exec` detected")
    }
}

/// S102
pub(crate) fn exec_used(checker: &mut Checker, func: &Expr) {
    if checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["" | "builtin", "exec"]))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(ExecBuiltin, func.range()));
    }
}
