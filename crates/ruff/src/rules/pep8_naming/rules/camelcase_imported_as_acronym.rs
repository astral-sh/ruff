use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string::{self};
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::rules::pep8_naming::helpers;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for [`CamelCase`] imports that are aliased as acronyms.
    ///
    /// ## Why is this bad?
    /// [PEP8] recommends naming conventions for class names,
    /// function names, constants etc. Inconsistenct [naming styles]
    /// between import name and the alias may lead developers to expect an import to be of another
    /// type (e.g. a confuse class with a constant).
    ///
    /// Importing aliases should thus follow the same naming conventions.
    ///
    /// This rule is distinct from 'camelcase-imported-as-constant' for selective enforcement.
    ///
    /// ## Example
    /// ```python
    /// from example import MyClassName as MCN
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

    pub struct CamelcaseImportedAsAcronym {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for CamelcaseImportedAsAcronym {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsAcronym { name, asname } = self;
        format!("Camelcase `{name}` imported as acronym `{asname}`")
    }
}

/// N817
pub fn camelcase_imported_as_acronym(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if helpers::is_camelcase(name)
        && !string::is_lower(asname)
        && string::is_upper(asname)
        && helpers::is_acronym(name, asname)
    {
        return Some(Diagnostic::new(
            CamelcaseImportedAsAcronym {
                name: name.to_string(),
                asname: asname.to_string(),
            },
            identifier_range(import_from, locator),
        ));
    }
    None
}
