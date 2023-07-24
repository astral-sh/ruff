use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_semantic::Binding;

/// ## What it does
/// Checks for the presence of unused private `TypeVar` declarations.
///
/// ## Why is this bad?
/// A private `TypeVar` that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// ## Example
/// ```python
/// import typing
/// _T = typing.TypeVar("_T")
/// ```
///
/// Use instead:
/// ```python
/// import typing
/// _T = typing.TypeVar("_T")
///
/// def func(arg: _T) -> _T: ...
/// ```
#[violation]
pub struct UnusedPrivateTypeVar {
    name: String,
}

impl Violation for UnusedPrivateTypeVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypeVar { name } = self;
        format!("TypeVar `{name}` is never used")
    }
}

/// PYI018
pub(crate) fn unused_private_type_var(binding: &Binding, locator: &Locator) -> Option<Diagnostic> {
    if !binding.kind.is_assignment() {
        return None;
    }
    if !binding.is_private_type_var() {
        return None;
    }
    if binding.is_used() {
        return None;
    }

    Some(Diagnostic::new(
        UnusedPrivateTypeVar {
            name: binding.name(locator).to_string(),
        },
        binding.range,
    ))
}
