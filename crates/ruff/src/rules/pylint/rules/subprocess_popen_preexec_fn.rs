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
pub struct SubprocessPopenPreexecFn;

impl Violation for SubprocessPopenPreexecFn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("TODO: write message: {}", todo!("implement message"))
    }
}

/// PLW1509
pub(crate) fn subprocess_popen_preexec_fn(checker: &mut Checker) {}
