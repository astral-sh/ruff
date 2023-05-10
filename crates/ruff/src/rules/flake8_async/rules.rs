use ruff_diagnostics::Violation;
use crate::registry::{AsRule, Rule};

// TODO: Fix docstrings and improve messages

/// ## What it does
/// Checks that async functions do not contain a blocking HTTP call
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// Use instead:
/// TODO
#[violation]
pub struct BlockingHttpCallInsideAsyncDef;

impl Violation for BlockingHttpCallInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call a blocking HTTP method")
    }
}
/// ASY100
// TODO: Implement
// pub fn blocking_http_call_inside_async_def {}


/// ## What it does
/// Checks that async functions do not contain a call to `open`, `time.sleep` or `subprocess`
/// methods
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// Use instead:
/// TODO
#[violation]
pub struct OpenSleepOrSubprocessInsideAsyncDef;

impl Violation for OpenSleepOrSubprocessInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Async functions should not contain a call to `open`, `time.sleep` or any \
             subprocess method"
        )
    }
}

/// ASY101
// TODO: Implement
// pub fn open_sleep_or_subprocess_inside_async_def {}

/// ## What it does
/// Checks that async functions do not contain a call to an unsafe `os` method
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// Use instead:
/// TODO
#[violation]
pub struct UnsafeOsMethodInsideAsyncDef;

impl Violation for UnsafeOsMethodInsideAsyncDef {
    fn message(&self) -> String {
        format!("Async functions should not contain a call to unsafe `os` methods")
    }
}

/// ASY102
// TODO: Implement
// pub fn unsafe_os_method_inside_async_def {}
