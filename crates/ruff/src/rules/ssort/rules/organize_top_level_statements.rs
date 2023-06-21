use ruff_diagnostics::{AutofixKind, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Groups and sorts top-level statements based on the order in which they are
/// referenced.
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for statement ordering to make
/// your code more readable and idiomatic.
///
/// ## Example
/// ```python
/// def h():
///     g()
///
/// def f():
///     pass
///
/// def g():
///    f()
/// ```
///
/// Use instead:
/// ```python
/// def f():
///     pass
///
/// def g():
///    f()
///
/// def h():
///     g()
/// ```
#[violation]
pub struct UnsortedTopLevelStatements;

impl Violation for UnsortedTopLevelStatements {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Top level statements are un-sorted")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Organize top-level statements".to_string())
    }
}
