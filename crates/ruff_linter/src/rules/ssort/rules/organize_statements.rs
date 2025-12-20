use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::{FixAvailability, Violation};

/// ## What it does
///
/// Groups and sorts statements based on the order in which they are referenced.
///
/// ## Why is this bad?
///
/// Consistency is good. Use a common convention for statement ordering to make your code more
/// readable and idiomatic.
///
/// ## Example
///
/// ```python
/// def foo():
///     bar()
///
///
/// def baz():
///     pass
///
///
/// def bar():
///     baz()
/// ```
///
/// Use instead:
///
/// ```python
/// def baz():
///     pass
///
///
/// def bar():
///     baz()
///
///
/// def foo():
///     bar()
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct UnsortedStatements;

/// Allows UnsortedStatements to be treated as a Violation.
impl Violation for UnsortedStatements {
    /// Fix is sometimes available.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    /// The message used to describe the violation.
    ///
    /// ## Returns
    /// A string describing the violation.
    #[derive_message_formats]
    fn message(&self) -> String {
        "Statements are unsorted".to_string()
    }

    /// Returns the title for the fix.
    ///
    /// ## Returns
    /// A string describing the fix title.
    fn fix_title(&self) -> Option<String> {
        Some("Organize statements".to_string())
    }
}

/// SS001
pub(crate) fn organize_statements() {}
