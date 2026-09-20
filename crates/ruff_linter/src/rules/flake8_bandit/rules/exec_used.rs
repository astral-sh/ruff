use ruff_python_ast::Expr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::preview::is_exec_used_reference_enabled;

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
/// In [preview], this rule will also flag references to the `exec` builtin
/// that are not calls, mirroring the reference behavior of
/// [`suspicious-eval-usage`] (S307). For example:
///
/// ```python
/// list(map(exec, ["print('Hello World')"]))
/// ```
///
/// [preview]: https://docs.astral.sh/ruff/preview/
/// [`suspicious-eval-usage`]: https://docs.astral.sh/ruff/rules/suspicious-eval-usage/
///
/// ## References
/// - [Python documentation: `exec`](https://docs.python.org/3/library/functions.html#exec)
/// - [Common Weakness Enumeration: CWE-78](https://cwe.mitre.org/data/definitions/78.html)
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

/// S102 (reference form)
///
/// In [preview], this rule will also flag references to the `exec` builtin, mirroring the
/// reference behavior of `suspicious-eval-usage` (S307).
///
/// [preview]: https://docs.astral.sh/ruff/preview/
pub(crate) fn exec_used_reference(checker: &Checker, func: &Expr, semantic: &SemanticModel) {
    if !is_exec_used_reference_enabled(checker.settings()) {
        return;
    }

    if !semantic.match_builtin_expr(func, "exec") {
        return;
    }

    // Avoid a duplicate diagnostic when the name is the call target, e.g.
    // `exec("...")`, which `exec_used` already reports.
    if let Some(parent) = semantic.current_expression_parent() {
        if let Expr::Call(call) = parent {
            if call.func.range().contains_range(func.range()) {
                return;
            }
        }
    }

    checker.report_diagnostic(ExecBuiltin, func.range());
}
