use ruff_diagnostics::{Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextSize;

use crate::registry::Rule;

pub(crate) const TEST_RULES: &[Rule] = &[
    Rule::StableTestRule,
    Rule::StableTestRuleSafeFix,
    Rule::StableTestRuleUnsafeFix,
    Rule::StableTestRuleDisplayOnlyFix,
    Rule::NurseryTestRule,
    Rule::PreviewTestRule,
];

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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

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
pub struct StableTestRuleSafeFix;

impl Violation for StableTestRuleSafeFix {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a stable test rule with a safe fix.")
    }
}

impl StableTestRuleSafeFix {
    pub(crate) fn fix() -> Fix {
        Fix::safe_edit(Edit::insertion(
            "# safe insertion\n".to_string(),
            TextSize::new(0),
        ))
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
pub struct StableTestRuleUnsafeFix;

impl Violation for StableTestRuleUnsafeFix {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a stable test rule with an unsafe fix.")
    }
}

impl StableTestRuleUnsafeFix {
    pub(crate) fn fix() -> Fix {
        Fix::unsafe_edit(Edit::insertion(
            "# unsafe insertion\n".to_string(),
            TextSize::new(0),
        ))
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
pub struct StableTestRuleDisplayOnlyFix;

impl Violation for StableTestRuleDisplayOnlyFix {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a stable test rule with a display only fix.")
    }
}

impl StableTestRuleDisplayOnlyFix {
    pub(crate) fn fix() -> Fix {
        Fix::display_only_edit(Edit::insertion(
            "# display only insertion\n".to_string(),
            TextSize::new(0),
        ))
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a nursery test rule.")
    }
}
