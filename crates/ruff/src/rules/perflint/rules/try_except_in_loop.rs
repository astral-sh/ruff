use rustpython_parser::ast::{self, Ranged, Stmt};

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
/// for i in range(10):
///     try:
///         print(i * i)
///     except:
///         break
/// ```
///
/// Use instead:
/// ```python
/// try:
///     for i in range(10):
///         print(i * i)
/// except:
///     break
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

    checker.diagnostics.extend(body.iter().filter_map(|stmt| {
        if let Stmt::Try(ast::StmtTry { handlers, .. }) = stmt {
            handlers
                .iter()
                .next()
                .map(|handler| Diagnostic::new(TryExceptInLoop, handler.range()))
        } else {
            None
        }
    }));
}
