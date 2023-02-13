use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string;
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for constant imports that are aliased as non constant.
    ///
    /// ## Why is this bad?
    /// [PEP8] recommends naming conventions for class names,
    /// function names, constants etc. Inconsistenct [naming styles]
    /// between import name and the alias may lead developers to expect an import to be of another
    /// type (e.g. a confuse class with a constant).
    ///
    /// Importing aliases should thus follow the same naming conventions.
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
    /// [PEP8]: https://peps.python.org/pep-0008/
    /// [naming styles]: https://peps.python.org/pep-0008/#descriptive-naming-styles
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
