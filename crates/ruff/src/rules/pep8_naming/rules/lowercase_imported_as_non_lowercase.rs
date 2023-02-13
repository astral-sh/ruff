use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string;
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for lowercase imports that are aliased as non-lowercase.
    ///
    /// ## Why is this bad?
    /// [PEP8] recommends naming conventions for class names,
    /// function names, constants etc. Inconsistenct [naming styles]
    /// between import name and the alias may lead developers to expect an import to be of another
    /// type (e.g. confuse a class with a function).
    ///
    /// Importing aliases should thus follow the same naming conventions.
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
    /// [PEP8]: https://peps.python.org/pep-0008/
    /// [naming styles]: https://peps.python.org/pep-0008/#descriptive-naming-styles
    ///
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
