use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::{FixAvailability, Violation};

/// ## Removed
/// This rule is identical to [G010] which should be used instead.
///
/// ## What it does
/// Check for usages of the deprecated `warn` method from the `logging` module.
///
/// ## Why is this bad?
/// The `warn` method is deprecated. Use `warning` instead.
///
/// ## Example
/// ```python
/// import logging
///
///
/// def foo():
///     logging.warn("Something happened")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
///
/// def foo():
///     logging.warning("Something happened")
/// ```
///
/// ## References
/// - [Python documentation: `logger.Logger.warning`](https://docs.python.org/3/library/logging.html#logging.Logger.warning)
///
/// [G010]: https://docs.astral.sh/ruff/rules/logging-warn/
#[derive(ViolationMetadata)]
#[violation_metadata(removed_since = "v0.2.0")]
pub(crate) struct DeprecatedLogWarn;

/// PGH002
impl Violation for DeprecatedLogWarn {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`warn` is deprecated in favor of `warning`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `warning`".to_string())
    }
}
