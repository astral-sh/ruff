use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for invalid assignments to `__all__`.
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
/// __all__ = "Foo"
/// ```
///
/// Use instead:
/// ```python
/// __all__ = ("Foo",)
/// ```
///
/// ## References
/// - [Python documentation: The `import` statement](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
#[violation]
pub struct InvalidAllFormat;

impl Violation for InvalidAllFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid format for `__all__`, must be `tuple` or `list`")
    }
}

/// PLE0605
pub(crate) fn invalid_all_format(binding: &Binding) -> Option<Diagnostic> {
    if binding.is_invalid_all_format() {
        Some(Diagnostic::new(InvalidAllFormat, binding.range()))
    } else {
        None
    }
}
