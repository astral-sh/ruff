use ruff_diagnostics::{AutofixKind, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Fake rule for testing.
///
/// ## Why is this bad?
/// Tests must pass!
///
/// ## Example
/// ```python
/// foo
/// ```
///
/// Use instead:
/// ```python
/// bar
/// ```
#[violation]
pub struct StableTestRule;

impl Violation for StableTestRule {
    const AUTOFIX: AutofixKind = AutofixKind::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a stable test rule.")
    }
}

/// ## What it does
/// Fake rule for testing.
///
/// ## Why is this bad?
/// Tests must pass!
///
/// ## Example
/// ```python
/// foo
/// ```
///
/// Use instead:
/// ```python
/// bar
/// ```
#[violation]
pub struct PreviewTestRule;

impl Violation for PreviewTestRule {
    const AUTOFIX: AutofixKind = AutofixKind::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a preview test rule.")
    }
}

/// ## What it does
/// Fake rule for testing.
///
/// ## Why is this bad?
/// Tests must pass!
///
/// ## Example
/// ```python
/// foo
/// ```
///
/// Use instead:
/// ```python
/// bar
/// ```
#[violation]
pub struct NurseryTestRule;

impl Violation for NurseryTestRule {
    const AUTOFIX: AutofixKind = AutofixKind::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a nursery test rule.")
    }
}
