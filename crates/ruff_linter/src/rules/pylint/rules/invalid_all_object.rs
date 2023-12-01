use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for the inclusion of invalid objects in `__all__`.
///
/// ## Why is this bad?
/// In Python, `__all__` should contain a sequence of strings that represent
/// the names of all "public" symbols exported by a module.
///
/// Assigning anything other than a `tuple` or `list` of strings to `__all__`
/// is invalid.
///
/// ## Example
/// ```python
/// __all__ = [Foo, 1, None]
/// ```
///
/// Use instead:
/// ```python
/// __all__ = ["Foo", "Bar", "Baz"]
/// ```
///
/// ## References
/// - [Python documentation: The `import` statement](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
#[violation]
pub struct InvalidAllObject;

impl Violation for InvalidAllObject {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid object in `__all__`, must contain only strings")
    }
}

/// PLE0604
pub(crate) fn invalid_all_object(binding: &Binding) -> Option<Diagnostic> {
    if binding.is_invalid_all_object() {
        Some(Diagnostic::new(InvalidAllObject, binding.range()))
    } else {
        None
    }
}
