use std::fmt;

use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DeferralKeyword {
    Yield,
    YieldFrom,
    Await,
}

impl fmt::Display for DeferralKeyword {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeferralKeyword::Yield => fmt.write_str("yield"),
            DeferralKeyword::YieldFrom => fmt.write_str("yield from"),
            DeferralKeyword::Await => fmt.write_str("await"),
        }
    }
}

/// ## What it does
/// Checks for `yield`, `yield from`, and `await` usages outside of functions.
///
/// ## Why is this bad?
/// The use of `yield`, `yield from`, or `await` outside of a function will
/// raise a `SyntaxError`.
///
/// ## Example
/// ```python
/// class Foo:
///     yield 1
/// ```
///
/// ## Notebook behavior
/// As an exception, `await` is allowed at the top level of a Jupyter notebook
/// (see: [autoawait]).
///
/// ## References
/// - [Python documentation: `yield`](https://docs.python.org/3/reference/simple_stmts.html#the-yield-statement)
///
/// [autoawait]: https://ipython.readthedocs.io/en/stable/interactive/autoawait.html
#[derive(ViolationMetadata)]
pub(crate) struct YieldOutsideFunction {
    pub(crate) keyword: DeferralKeyword,
}

impl Violation for YieldOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let YieldOutsideFunction { keyword } = self;
        format!("`{keyword}` statement outside of a function")
    }
}
