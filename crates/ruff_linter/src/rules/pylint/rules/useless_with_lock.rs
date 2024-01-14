use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for direct uses of lock objects in `with` statements.
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
#[violation]
pub struct UselessWithLock;

impl Violation for UselessWithLock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Threading lock directly created in `with` statement has no effect")
    }
}

/// PLW2101
pub(crate) fn useless_with_lock(checker: &mut Checker, with: &ast::StmtWith) {
    for item in &with.items {
        let Some(call) = item.context_expr.as_call_expr() else {
            continue;
        };

        if !checker
            .semantic()
            .resolve_call_path(call.func.as_ref())
            .is_some_and(|call_path| {
                matches!(
                    call_path.as_slice(),
                    [
                        "threading",
                        "Lock" | "RLock" | "Condition" | "Semaphore" | "BoundedSemaphore"
                    ]
                )
            })
        {
            return;
        }

        checker
            .diagnostics
            .push(Diagnostic::new(UselessWithLock, call.range()));
    }
}
