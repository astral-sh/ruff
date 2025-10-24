use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for `__future__` imports that are not defined in the current Python
/// version.
///
/// ## Why is this bad?
/// Importing undefined or unsupported members from the `__future__` module is
/// a `SyntaxError`.
///
/// ## References
/// - [Python documentation: `__future__`](https://docs.python.org/3/library/__future__.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.34")]
pub(crate) struct FutureFeatureNotDefined {
    pub name: String,
}

impl Violation for FutureFeatureNotDefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureFeatureNotDefined { name } = self;
        format!("Future feature `{name}` is not defined")
    }
}
