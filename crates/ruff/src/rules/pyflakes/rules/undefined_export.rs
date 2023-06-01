use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::Scope;

/// ## What it does
/// Checks for undefined names in `__all__`.
///
/// ## Why is this bad?
/// `__all__` is used to define the names exported by a module. An undefined
/// name in `__all__` is likely to raise `NameError` at runtime when the module
/// is imported.
///
/// ## Example
/// ```python
/// from foo import bar
///
/// __all__ = ["bar", "baz"]  # undefined name `baz` in `__all__`
/// ```
///
/// Use instead:
/// ```python
/// from foo import bar, baz
///
/// __all__ = ["bar", "baz"]
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
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
pub(crate) fn undefined_export(names: &[&str], range: TextRange, scope: &Scope) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if !scope.uses_star_imports() {
        for name in names {
            if !scope.defines(name) {
                diagnostics.push(Diagnostic::new(
                    UndefinedExport {
                        name: (*name).to_string(),
                    },
                    range,
                ));
            }
        }
    }
    diagnostics
}
