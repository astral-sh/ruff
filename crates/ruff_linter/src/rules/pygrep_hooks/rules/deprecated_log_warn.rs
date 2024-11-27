use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

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
