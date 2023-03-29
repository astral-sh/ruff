use rustpython_parser::ast::{Alias, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_stdlib::str;

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
    pub name: String,
    pub asname: String,
}

impl Violation for ConstantImportedAsNonConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConstantImportedAsNonConstant { name, asname } = self;
        format!("Constant `{name}` imported as non-constant `{asname}`")
    }
}

/// N811
pub fn constant_imported_as_non_constant(
    name: &str,
    asname: &str,
    alias: &Alias,
    stmt: &Stmt,
) -> Option<Diagnostic> {
    if str::is_upper(name) && !str::is_upper(asname) {
        let mut diagnostic = Diagnostic::new(
            ConstantImportedAsNonConstant {
                name: name.to_string(),
                asname: asname.to_string(),
            },
            Range::from(alias),
        );
        diagnostic.set_parent(stmt.location);
        return Some(diagnostic);
    }
    None
}
