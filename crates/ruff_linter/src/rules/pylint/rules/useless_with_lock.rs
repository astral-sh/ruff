use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for lock objects that are created and immediately discarded in
/// `with` statements.
///
/// ## Why is this bad?
/// Creating a lock (via `threading.Lock` or similar) in a `with` statement
/// has no effect, as locks are only relevant when shared between threads.
///
/// Instead, assign the lock to a variable outside the `with` statement,
/// and share that variable between threads.
///
/// ## Example
/// ```python
/// import threading
///
/// counter = 0
///
///
/// def increment():
///     global counter
///
///     with threading.Lock():
///         counter += 1
/// ```
///
/// Use instead:
/// ```python
/// import threading
///
/// counter = 0
/// lock = threading.Lock()
///
///
/// def increment():
///     global counter
///
///     with lock:
///         counter += 1
/// ```
///
/// ## References
/// - [Python documentation: `Lock Objects`](https://docs.python.org/3/library/threading.html#lock-objects)
#[derive(ViolationMetadata)]
pub(crate) struct UselessWithLock;

impl Violation for UselessWithLock {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Threading lock directly created in `with` statement has no effect".to_string()
    }
}

/// PLW2101
pub(crate) fn useless_with_lock(checker: &Checker, with: &ast::StmtWith) {
    for item in &with.items {
        let Some(call) = item.context_expr.as_call_expr() else {
            continue;
        };

        if !checker
            .semantic()
            .resolve_qualified_name(call.func.as_ref())
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    [
                        "threading",
                        "Lock" | "RLock" | "Condition" | "Semaphore" | "BoundedSemaphore"
                    ]
                )
            })
        {
            return;
        }

        checker.report_diagnostic(Diagnostic::new(UselessWithLock, call.range()));
    }
}
