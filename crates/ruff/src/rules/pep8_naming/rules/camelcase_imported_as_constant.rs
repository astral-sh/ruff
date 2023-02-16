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
    /// Checks for `CamelCase` imports that are aliased to constant-style names.
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
    /// from example import MyClassName as MY_CLASS_NAME
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from example import MyClassName
    /// ```
    ///
    /// [PEP 8]: https://peps.python.org/pep-0008/
    pub struct CamelcaseImportedAsConstant {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for CamelcaseImportedAsConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsConstant { name, asname } = self;
        format!("Camelcase `{name}` imported as constant `{asname}`")
    }
}

/// N814
pub fn camelcase_imported_as_constant(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if helpers::is_camelcase(name)
        && !string::is_lower(asname)
        && string::is_upper(asname)
        && !helpers::is_acronym(name, asname)
    {
        return Some(Diagnostic::new(
            CamelcaseImportedAsConstant {
                name: name.to_string(),
                asname: asname.to_string(),
            },
            identifier_range(import_from, locator),
        ));
    }
    None
}
