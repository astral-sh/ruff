use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## Removed
/// This rule is identical to [B904] which should be used instead.
///
/// ## What it does
/// Checks for exceptions that are re-raised without specifying the cause via
/// the `from` keyword.
///
/// ## Why is this bad?
/// The `from` keyword sets the `__cause__` attribute of the exception, which
/// stores the "cause" of the exception. The availability of an exception
/// "cause" is useful for debugging.
///
/// ## Example
/// ```python
/// def reciprocal(n):
///     try:
///         return 1 / n
///     except ZeroDivisionError:
///         raise ValueError()
/// ```
///
/// Use instead:
/// ```python
/// def reciprocal(n):
///     try:
///         return 1 / n
///     except ZeroDivisionError as exc:
///         raise ValueError() from exc
/// ```
///
/// ## References
/// - [Python documentation: Exception context](https://docs.python.org/3/library/exceptions.html#exception-context)
///
/// [B904]: https://docs.astral.sh/ruff/rules/raise-without-from-inside-except/
#[violation]
pub struct ReraiseNoCause;

/// TRY200
impl Violation for ReraiseNoCause {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `raise from` to specify exception cause")
    }
}
