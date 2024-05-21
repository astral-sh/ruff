use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
///
/// ## Why is this bad?
///
/// ## Example
/// ```python
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct SleepForeverCall;

impl Violation for SleepForeverCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("TODO: write message: {}", todo!("implement message"))
    }
}

/// ASYNC116
pub(crate) fn sleep_forever_call(checker: &mut Checker) {}
