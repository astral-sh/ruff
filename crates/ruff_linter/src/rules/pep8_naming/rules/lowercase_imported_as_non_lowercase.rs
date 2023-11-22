use ruff_python_ast::{Alias, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::str;
use ruff_text_size::Ranged;

use crate::settings::types::IdentifierPattern;

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
#[violation]
pub struct LowercaseImportedAsNonLowercase {
    name: String,
    asname: String,
}

impl Violation for LowercaseImportedAsNonLowercase {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LowercaseImportedAsNonLowercase { name, asname } = self;
        format!("Lowercase `{name}` imported as non-lowercase `{asname}`")
    }
}

/// N812
pub(crate) fn lowercase_imported_as_non_lowercase(
    name: &str,
    asname: &str,
    alias: &Alias,
    stmt: &Stmt,
    ignore_names: &[IdentifierPattern],
) -> Option<Diagnostic> {
    if ignore_names
        .iter()
        .any(|ignore_name| ignore_name.matches(asname))
    {
        return None;
    }

    if !str::is_cased_uppercase(name) && str::is_cased_lowercase(name) && !str::is_lowercase(asname)
    {
        let mut diagnostic = Diagnostic::new(
            LowercaseImportedAsNonLowercase {
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
