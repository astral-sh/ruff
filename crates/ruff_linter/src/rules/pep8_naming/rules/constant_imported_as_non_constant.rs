use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, Stmt};
use ruff_python_stdlib::str;
use ruff_text_size::Ranged;

use crate::rules::pep8_naming::settings::IgnoreNames;

/// ## What it does
/// Checks for constant imports that are aliased to non-constant-style
/// names.
///
/// ## Why is this bad?
/// [PEP 8] recommends naming conventions for classes, functions,
/// constants, and more. The use of inconsistent naming styles between
/// import and alias names may lead readers to expect an import to be of
/// another type (e.g., confuse a Python class with a constant).
///
/// Import aliases should thus follow the same naming style as the member
/// being imported.
///
/// ## Example
/// ```python
/// from example import CONSTANT_VALUE as ConstantValue
/// ```
///
/// Use instead:
/// ```python
/// from example import CONSTANT_VALUE
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/
#[violation]
pub struct ConstantImportedAsNonConstant {
    name: String,
    asname: String,
}

impl Violation for ConstantImportedAsNonConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConstantImportedAsNonConstant { name, asname } = self;
        format!("Constant `{name}` imported as non-constant `{asname}`")
    }
}

/// N811
pub(crate) fn constant_imported_as_non_constant(
    name: &str,
    asname: &str,
    alias: &Alias,
    stmt: &Stmt,
    ignore_names: &IgnoreNames,
) -> Option<Diagnostic> {
    if str::is_cased_uppercase(name) && !str::is_cased_uppercase(asname) {
        // Ignore any explicitly-allowed names.
        if ignore_names.matches(name) || ignore_names.matches(asname) {
            return None;
        }
        let mut diagnostic = Diagnostic::new(
            ConstantImportedAsNonConstant {
                name: name.to_string(),
                asname: asname.to_string(),
            },
            alias.range(),
        );
        diagnostic.set_parent(stmt.start());
        return Some(diagnostic);
    }
    None
}
