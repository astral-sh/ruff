use ruff_diagnostics::AlwaysFixableViolation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

use crate::fix::snippet::SourceCodeSnippet;

/// ## Removed
/// This rule is identical to [SIM101] which should be used instead.
///
/// ## What it does
/// Checks for repeated `isinstance` calls on the same object.
///
/// ## Why is this bad?
/// Repeated `isinstance` calls on the same object can be merged into a
/// single call.
///
/// ## Fix safety
/// This rule's fix is marked as unsafe on Python 3.10 and later, as combining
/// multiple `isinstance` calls with a binary operator (`|`) will fail at
/// runtime if any of the operands are themselves tuples.
///
/// For example, given `TYPES = (dict, list)`, then
/// `isinstance(None, TYPES | set | float)` will raise a `TypeError` at runtime,
/// while `isinstance(None, set | float)` will not.
///
/// ## Example
/// ```python
/// def is_number(x):
///     return isinstance(x, int) or isinstance(x, float) or isinstance(x, complex)
/// ```
///
/// Use instead:
/// ```python
/// def is_number(x):
///     return isinstance(x, (int, float, complex))
/// ```
///
/// Or, for Python 3.10 and later:
///
/// ```python
/// def is_number(x):
///     return isinstance(x, int | float | complex)
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `isinstance`](https://docs.python.org/3/library/functions.html#isinstance)
///
/// [SIM101]: https://docs.astral.sh/ruff/rules/duplicate-isinstance-call/
#[derive(ViolationMetadata)]
pub(crate) struct RepeatedIsinstanceCalls {
    expression: SourceCodeSnippet,
}

/// PLR1701
impl AlwaysFixableViolation for RepeatedIsinstanceCalls {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(expression) = self.expression.full_display() {
            format!("Merge `isinstance` calls: `{expression}`")
        } else {
            "Merge `isinstance` calls".to_string()
        }
    }

    fn fix_title(&self) -> String {
        if let Some(expression) = self.expression.full_display() {
            format!("Replace with `{expression}`")
        } else {
            "Replace with merged `isinstance` call".to_string()
        }
    }
}
