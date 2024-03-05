use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## Removed
/// This rule was implemented in `flake8-bugbear` and has been remapped to [B035]
///
/// ## What it does
/// Checks for dictionary comprehensions that use a static key, like a string
/// literal or a variable defined outside the comprehension.
///
/// ## Why is this bad?
/// Using a static key (like a string literal) in a dictionary comprehension
/// is usually a mistake, as it will result in a dictionary with only one key,
/// despite the comprehension iterating over multiple values.
///
/// ## Example
/// ```python
/// data = ["some", "Data"]
/// {"key": value.upper() for value in data}
/// ```
///
/// Use instead:
/// ```python
/// data = ["some", "Data"]
/// {value: value.upper() for value in data}
/// ```
///
/// [B035]: https://docs.astral.sh/ruff/rules/static-key-dict-comprehension/
#[violation]
pub struct RuffStaticKeyDictComprehension;

impl Violation for RuffStaticKeyDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Dictionary comprehension uses static key")
    }
}
