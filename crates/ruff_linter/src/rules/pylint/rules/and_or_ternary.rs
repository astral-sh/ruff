use ruff_diagnostics::Violation;
use ruff_macros::ViolationMetadata;

/// ## Removal
/// This rule was removed from Ruff because it was common for it to introduce behavioral changes.
/// See [#9007](https://github.com/astral-sh/ruff/issues/9007) for more information.
///
/// ## What it does
/// Checks for uses of the known pre-Python 2.5 ternary syntax.
///
/// ## Why is this bad?
/// Prior to the introduction of the if-expression (ternary) operator in Python
/// 2.5, the only way to express a conditional expression was to use the `and`
/// and `or` operators.
///
/// The if-expression construct is clearer and more explicit, and should be
/// preferred over the use of `and` and `or` for ternary expressions.
///
/// ## Example
/// ```python
/// x, y = 1, 2
/// maximum = x >= y and x or y
/// ```
///
/// Use instead:
/// ```python
/// x, y = 1, 2
/// maximum = x if x >= y else y
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AndOrTernary;

/// PLR1706
impl Violation for AndOrTernary {
    fn message(&self) -> String {
        unreachable!("PLR1706 has been removed");
    }

    fn message_formats() -> &'static [&'static str] {
        &["Consider using if-else expression"]
    }
}
