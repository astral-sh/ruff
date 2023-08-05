use ruff_python_ast::{self as ast, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for uses of except handling via `try`-`except` within `for` and
/// `while` loops.
///
/// ## Why is this bad?
/// Exception handling via `try`-`except` blocks incurs some performance
/// overhead, regardless of whether an exception is raised.
///
/// When possible, refactor your code to put the entire loop into the
/// `try`-`except` block, rather than wrapping each iteration in a separate
/// `try`-`except` block.
///
/// This rule is only enforced for Python versions prior to 3.11, which
/// introduced "zero cost" exception handling.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// string_numbers: list[str] = ["1", "2", "three", "4", "5"]
///
/// int_numbers: list[int] = []
/// for num in string_numbers:
///     try:
///         int_numbers.append(int(num))
///     except ValueError as e:
///         print(f"Couldn't convert to integer: {e}")
/// ```
///
/// Use instead:
/// ```python
/// string_numbers: list[str] = ["1", "2", "three", "4", "5"]
///
/// int_numbers: list[int] = []
/// try:
///     for num in string_numbers:
///         int_numbers.append(int(num))
/// except ValueError as e:
///     print(f"Couldn't convert to integer: {e}")
/// ```
///
/// ## Options
/// - `target-version`
#[violation]
pub struct TryExceptInLoop;

impl Violation for TryExceptInLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`try`-`except` within a loop incurs performance overhead")
    }
}

/// PERF203
pub(crate) fn try_except_in_loop(checker: &mut Checker, body: &[Stmt]) {
    if checker.settings.target_version >= PythonVersion::Py311 {
        return;
    }

    let [Stmt::Try(ast::StmtTry { handlers, .. })] = body else {
        return;
    };

    let Some(handler) = handlers.first() else {
        return;
    };

    checker
        .diagnostics
        .push(Diagnostic::new(TryExceptInLoop, handler.range()));
}
