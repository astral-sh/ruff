use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;
use ruff_python_stdlib::str::{self};

use crate::rules::pep8_naming::helpers;

/// ## What it does
/// Checks for `CamelCase` imports that are aliased as acronyms.
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
/// Note that this rule is distinct from `camelcase-imported-as-constant`
/// to accommodate selective enforcement.
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
/// [PEP 8]: https://peps.python.org/pep-0008/
#[violation]
pub struct CamelcaseImportedAsAcronym {
    pub name: String,
    pub asname: String,
}

impl Violation for CamelcaseImportedAsAcronym {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsAcronym { name, asname } = self;
        format!("CamelCase `{name}` imported as acronym `{asname}`")
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
        && !str::is_lower(asname)
        && str::is_upper(asname)
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
