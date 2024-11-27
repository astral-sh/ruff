use ruff_diagnostics::Violation;
use ruff_macros::ViolationMetadata;

/// ## Removed
/// There's no [evidence](https://github.com/astral-sh/ruff/issues/12754) that generators are
/// meaningfully faster than list comprehensions when combined with unpacking.
///
/// ## What it does
/// Checks for list comprehensions that are immediately unpacked.
///
/// ## Why is this bad?
/// There is no reason to use a list comprehension if the result is immediately
/// unpacked. Instead, use a generator expression, which avoids allocating
/// an intermediary list.
///
/// ## Example
/// ```python
/// a, b, c = [foo(x) for x in items]
/// ```
///
/// Use instead:
/// ```python
/// a, b, c = (foo(x) for x in items)
/// ```
///
/// ## References
/// - [Python documentation: Generator expressions](https://docs.python.org/3/reference/expressions.html#generator-expressions)
/// - [Python documentation: List comprehensions](https://docs.python.org/3/tutorial/datastructures.html#list-comprehensions)
#[derive(ViolationMetadata)]
pub(crate) struct UnpackedListComprehension;

impl Violation for UnpackedListComprehension {
    fn message(&self) -> String {
        unreachable!("UP027 has been removed")
    }

    fn message_formats() -> &'static [&'static str] {
        &["Replace unpacked list comprehension with a generator expression"]
    }
}
