use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string;
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for lowercase imports that are aliased to non-lowercase names.
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
    /// from example import myclassname as MyClassName
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from example import myclassname
    /// ```
    ///
    /// [PEP 8]: https://peps.python.org/pep-0008/
    pub struct LowercaseImportedAsNonLowercase {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for LowercaseImportedAsNonLowercase {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LowercaseImportedAsNonLowercase { name, asname } = self;
        format!("Lowercase `{name}` imported as non-lowercase `{asname}`")
    }
}

/// N812
pub fn lowercase_imported_as_non_lowercase(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if !string::is_upper(name) && string::is_lower(name) && asname.to_lowercase() != asname {
        return Some(Diagnostic::new(
            LowercaseImportedAsNonLowercase {
                name: name.to_string(),
                asname: asname.to_string(),
            },
            identifier_range(import_from, locator),
        ));
    }
    None
}
