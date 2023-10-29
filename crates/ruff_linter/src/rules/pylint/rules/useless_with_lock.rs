use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for useless lock objects in `with` statements.
///
/// ## Why is this bad?
/// Lock objects must be stored in a variable to be shared between threads.
/// Otherwise, each thread will have its own lock object and the lock will be useless.
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
        format!(
            "Useless lock object. Create a variable to store the lock object \
             and use it in `with` statement."
        )
    }
}

/// PLW2101
pub(crate) fn useless_with_lock(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().current_statement().is_with_stmt() {
        return;
    }

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
        .push(Diagnostic::new(UselessWithLock {}, call.range()));
}
