use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
#[derive(ViolationMetadata)]
pub(crate) struct ExecBuiltin;

impl Violation for ExecBuiltin {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `exec` detected".to_string()
    }
}

/// S102
pub(crate) fn exec_used(checker: &Checker, func: &Expr) {
    if checker.semantic().match_builtin_expr(func, "exec") {
        checker.report_diagnostic(Diagnostic::new(ExecBuiltin, func.range()));
    }
}
