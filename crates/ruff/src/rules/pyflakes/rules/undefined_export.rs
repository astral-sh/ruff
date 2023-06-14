use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Scope;

/// ## What it does
/// Checks for undefined names in `__all__`.
///
/// ## Why is this bad?
/// In Python, the `__all__` variable is used to define the names that are
/// exported when a module is imported as a wildcard (e.g.,
/// `from foo import *`). The names in `__all__` must be defined in the module,
/// but are included as strings.
///
/// Including an undefined name in `__all__` is likely to raise `NameError` at
/// runtime, when the module is imported.
///
/// ## Example
/// ```python
/// from foo import bar
///
///
/// __all__ = ["bar", "baz"]  # undefined name `baz` in `__all__`
/// ```
///
/// Use instead:
/// ```python
/// from foo import bar, baz
///
///
/// __all__ = ["bar", "baz"]
/// ```
///
/// ## References
/// - [Python documentation: `__all__`](https://docs.python.org/3/tutorial/modules.html#importing-from-a-package)
#[violation]
pub struct UndefinedExport {
    name: String,
}

impl Violation for UndefinedExport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedExport { name } = self;
        format!("Undefined name `{name}` in `__all__`")
    }
}

/// F822
pub(crate) fn undefined_export(name: &str, range: TextRange, scope: &Scope) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if !scope.uses_star_imports() {
        if !scope.has(name) {
            diagnostics.push(Diagnostic::new(
                UndefinedExport {
                    name: (*name).to_string(),
                },
                range,
            ));
        }
    }
    diagnostics
}
