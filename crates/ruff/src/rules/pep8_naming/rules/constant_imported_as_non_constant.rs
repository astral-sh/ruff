use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string;
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
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
    pub struct ConstantImportedAsNonConstant {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for ConstantImportedAsNonConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConstantImportedAsNonConstant { name, asname } = self;
        format!("Constant `{name}` imported as non-constant `{asname}`")
    }
}

/// N811
pub fn constant_imported_as_non_constant(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if string::is_upper(name) && !string::is_upper(asname) {
        return Some(Diagnostic::new(
            ConstantImportedAsNonConstant {
                name: name.to_string(),
                asname: asname.to_string(),
            },
            identifier_range(import_from, locator),
        ));
    }
    None
}
