use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `trio.sleep()` with >24 hour interval.
///
/// ## Why is this bad?
/// `trio.sleep()` with a >24 hour interval is usually intended to sleep indefintely.
/// This intent is be better conveyed using `trio.sleep_forever()`.
///
/// ## Example
/// ```python
/// import trio
///
///
/// async def func():
///     await trio.sleep(86401)
/// ```
///
/// Use instead:
/// ```python
/// import trio
///
///
/// async def func():
///     await trio.sleep_forever()
/// ```
#[violation]
pub struct SleepForeverCall;

impl Violation for SleepForeverCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`trio.sleep()` with >24 hour interval should usually be `trio.sleep_forever()`.")
    }
}

/// ASYNC116
pub(crate) fn sleep_forever_call(checker: &mut Checker) {}
