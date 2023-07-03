use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for uses of builtin `exec` function.
///
/// ## Why is this bad?
/// The `exec()` function is insecure as it enables arbitrary code execution.
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
pub(crate) fn exec_used(expr: &Expr, func: &Expr) -> Option<Diagnostic> {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return None;
    };
    if id != "exec" {
        return None;
    }
    Some(Diagnostic::new(ExecBuiltin, expr.range()))
}
