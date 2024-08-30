/// Fake rules for testing Ruff's behavior
///
/// All of these rules should be assigned to the RUF9XX codes.
///
/// Implementing a new test rule involves:
///
/// - Writing an empty struct for the rule
/// - Adding to the rule registry
/// - Adding to the `TEST_RULES` constant
/// - Implementing `Violation` for the rule
/// - Implementing `TestRule` for the rule
/// - Adding a match arm in `linter::check_path`
///
/// Rules that provide a fix _must_ not raise unconditionally or the linter
/// will not converge.
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;
use ruff_text_size::TextSize;

use crate::registry::Rule;

/// Check if a comment exists anywhere in a given file
fn comment_exists(text: &str, locator: &Locator, comment_ranges: &CommentRanges) -> bool {
    for range in comment_ranges {
        let comment_text = locator.slice(range);
        if text.trim_end() == comment_text {
            return true;
        }
    }
    false
}

pub(crate) const TEST_RULES: &[Rule] = &[
    Rule::StableTestRule,
    Rule::StableTestRuleSafeFix,
    Rule::StableTestRuleUnsafeFix,
    Rule::StableTestRuleDisplayOnlyFix,
    Rule::PreviewTestRule,
    Rule::DeprecatedTestRule,
    Rule::AnotherDeprecatedTestRule,
    Rule::RemovedTestRule,
    Rule::AnotherRemovedTestRule,
    Rule::RedirectedFromTestRule,
    Rule::RedirectedToTestRule,
    Rule::RedirectedFromPrefixTestRule,
];

pub(crate) trait TestRule {
    fn diagnostic(locator: &Locator, comment_ranges: &CommentRanges) -> Option<Diagnostic>;
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
pub struct StableTestRule;

impl Violation for StableTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a stable test rule.")
    }
}

impl TestRule for StableTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            StableTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct StableTestRuleSafeFix;

impl Violation for StableTestRuleSafeFix {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a stable test rule with a safe fix.")
    }
}

impl TestRule for StableTestRuleSafeFix {
    fn diagnostic(locator: &Locator, comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        let comment = format!("# fix from stable-test-rule-safe-fix\n");
        if comment_exists(&comment, locator, comment_ranges) {
            None
        } else {
            Some(
                Diagnostic::new(StableTestRuleSafeFix, ruff_text_size::TextRange::default())
                    .with_fix(Fix::safe_edit(Edit::insertion(
                        comment.to_string(),
                        TextSize::new(0),
                    ))),
            )
        }
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

impl TestRule for StableTestRuleUnsafeFix {
    fn diagnostic(locator: &Locator, comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        let comment = format!("# fix from stable-test-rule-unsafe-fix\n");
        if comment_exists(&comment, locator, comment_ranges) {
            None
        } else {
            Some(
                Diagnostic::new(
                    StableTestRuleUnsafeFix,
                    ruff_text_size::TextRange::default(),
                )
                .with_fix(Fix::unsafe_edit(Edit::insertion(
                    comment.to_string(),
                    TextSize::new(0),
                ))),
            )
        }
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

impl TestRule for StableTestRuleDisplayOnlyFix {
    fn diagnostic(locator: &Locator, comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        let comment = format!("# fix from stable-test-rule-display-only-fix\n");
        if comment_exists(&comment, locator, comment_ranges) {
            None
        } else {
            Some(
                Diagnostic::new(
                    StableTestRuleDisplayOnlyFix,
                    ruff_text_size::TextRange::default(),
                )
                .with_fix(Fix::display_only_edit(Edit::insertion(
                    comment.to_string(),
                    TextSize::new(0),
                ))),
            )
        }
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

impl TestRule for PreviewTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            PreviewTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct DeprecatedTestRule;

impl Violation for DeprecatedTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a deprecated test rule.")
    }
}

impl TestRule for DeprecatedTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            DeprecatedTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct AnotherDeprecatedTestRule;

impl Violation for AnotherDeprecatedTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is another deprecated test rule.")
    }
}

impl TestRule for AnotherDeprecatedTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            AnotherDeprecatedTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct RemovedTestRule;

impl Violation for RemovedTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a removed test rule.")
    }
}

impl TestRule for RemovedTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            RemovedTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct AnotherRemovedTestRule;

impl Violation for AnotherRemovedTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a another removed test rule.")
    }
}

impl TestRule for AnotherRemovedTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            AnotherRemovedTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct RedirectedFromTestRule;

impl Violation for RedirectedFromTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a test rule that was redirected to another.")
    }
}

impl TestRule for RedirectedFromTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            RedirectedFromTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct RedirectedToTestRule;

impl Violation for RedirectedToTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a test rule that was redirected from another.")
    }
}

impl TestRule for RedirectedToTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            RedirectedToTestRule,
            ruff_text_size::TextRange::default(),
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
pub struct RedirectedFromPrefixTestRule;

impl Violation for RedirectedFromPrefixTestRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Hey this is a test rule that was redirected to another by prefix.")
    }
}

impl TestRule for RedirectedFromPrefixTestRule {
    fn diagnostic(_locator: &Locator, _comment_ranges: &CommentRanges) -> Option<Diagnostic> {
        Some(Diagnostic::new(
            RedirectedFromPrefixTestRule,
            ruff_text_size::TextRange::default(),
        ))
    }
}
