use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::FixAvailability;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `sorted()` to get the min and max values of a list.
///
/// ## Why is this bad?
/// Using `sorted()` to get the min and max values of a list is inefficient.
///
/// ## Example
/// ```python
/// nums = [3, 1, 4, 1, 5]
/// lowest = sorted(nums)[0]
/// highest = sorted(nums)[-1]
/// highest = sorted(nums, reverse=True)[0]
/// ```
///
/// Use instead:
/// ```python
/// nums = [3, 1, 4, 1, 5]
/// lowest = min(nums)
/// highest = max(nums)
/// ```
///
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3/library/functions.html#max)

#[violation]
pub struct SortedMinMax;

impl Violation for SortedMinMax {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `min` and `max` over `sorted()` to get the min and max values of a list")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `min` and `max`"))
    }
}

/// FURB192
pub(crate) fn sorted_min_max(checker: &mut Checker, expr: &Expr) {
    let diagnostic = Diagnostic::new(SortedMinMax, TextRange::default());
    checker.diagnostics.push(diagnostic);
}

// TODO:
// - Caveat reverse=True with -1 as unsafe
