use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Binding;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of unused private `typing.Protocol` definitions.
///
/// ## Why is this bad?
/// A private `typing.Protocol` that is defined but not used is likely a mistake, consider
/// making it public.
///
/// ## Example
/// ```python
/// import typing
///
///
/// class _PrivateProtocol(typing.Protocol):
///     foo: int
/// ```
///
/// Use instead:
/// ```python
/// import typing
///
///
/// class _PrivateProtocol(typing.Protocol):
///     foo: int
///
///
/// def func(arg: _PrivateProtocol) -> None:
///     ...
/// ```
#[violation]
pub struct UnusedPrivateProtocol {
    name: String,
}

impl Violation for UnusedPrivateProtocol {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateProtocol { name } = self;
        format!("Private protocol `{name}` is never used")
    }
}

/// PYI046
pub(crate) fn unused_private_protocol(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !binding.kind.is_class_definition() {
        return None;
    }
    if !binding.is_private_protocol() {
        return None;
    }
    if binding.is_used() {
        return None;
    }

    Some(Diagnostic::new(
        UnusedPrivateProtocol {
            name: binding.name(checker.locator()).to_string(),
        },
        binding.range,
    ))
}
