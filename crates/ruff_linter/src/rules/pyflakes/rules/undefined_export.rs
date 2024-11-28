use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

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
/// In [preview], this rule will flag undefined names in `__init__.py` file,
/// even if those names implicitly refer to other modules in the package. Users
/// that rely on implicit exports should disable this rule in `__init__.py`
/// files via [`lint.per-file-ignores`].
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
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct UndefinedExport {
    pub name: String,
}

impl Violation for UndefinedExport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedExport { name } = self;
        format!("Undefined name `{name}` in `__all__`")
    }
}
