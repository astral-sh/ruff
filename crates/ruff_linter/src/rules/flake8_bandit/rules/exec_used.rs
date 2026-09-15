use ruff_python_ast::Expr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::preview::is_exec_builtin_reference_enabled;

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
/// In [preview], this rule will also flag references to `exec`.
///
/// ## References
/// - [Python documentation: `exec`](https://docs.python.org/3/library/functions.html#exec)
/// - [Common Weakness Enumeration: CWE-78](https://cwe.mitre.org/data/definitions/78.html)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.116", category = Category::Security)]
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
        checker.report_diagnostic(ExecBuiltin, func.range());
    }
}

/// S102
pub(crate) fn exec_used_reference(checker: &Checker, expr: &Expr) {
    if !is_exec_builtin_reference_enabled(checker.settings()) {
        return;
    }

    match checker.semantic().current_expression_parent() {
        // Avoid duplicate diagnostics. The callee of a call expression is
        // already reported by `exec_used`. For example:
        //
        // ```python
        // # vvvvvvvvv Already reported as a call expression
        //   exec("...")
        // # ^^^^ Should not be reported again as a reference
        // ```
        Some(Expr::Call(parent)) if parent.func.range().contains_range(expr.range()) => {
            return;
        }
        Some(Expr::Attribute(_)) => {
            return;
        }
        _ => {}
    }

    if checker.semantic().match_builtin_expr(expr, "exec") {
        checker.report_diagnostic(ExecBuiltin, expr.range());
    }
}
