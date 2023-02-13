use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string;
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::rules::pep8_naming::helpers;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for [`CamelCase`] imports that are aliased as lowercase.
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
    /// from example import MyClassName as myclassname
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from example import MyClassName
    /// ```
    ///
    /// [PEP8]: https://peps.python.org/pep-0008/
    /// [naming styles]: https://peps.python.org/pep-0008/#descriptive-naming-styles
    /// [`CamelCase`]: https://en.wikipedia.org/wiki/Camel_case
    pub struct CamelcaseImportedAsLowercase {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for CamelcaseImportedAsLowercase {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsLowercase { name, asname } = self;
        format!("Camelcase `{name}` imported as lowercase `{asname}`")
    }
}

/// N813
pub fn camelcase_imported_as_lowercase(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if helpers::is_camelcase(name) && string::is_lower(asname) {
        return Some(Diagnostic::new(
            CamelcaseImportedAsLowercase {
                name: name.to_string(),
                asname: asname.to_string(),
            },
            identifier_range(import_from, locator),
        ));
    }
    None
}
