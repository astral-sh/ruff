use ruff_python_ast::{Alias, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_stdlib::str::{self};
use ruff_text_size::Ranged;

use crate::rules::pep8_naming::helpers;
use crate::rules::pep8_naming::settings::IgnoreNames;

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
/// ## Note
/// Identifiers consisting of a single uppercase character are ambiguous under
/// the rules of [PEP 8], which specifies `CamelCase` for classes and
/// `ALL_CAPS_SNAKE_CASE` for constants. Without a second character, it is not
/// possible to reliably guess whether the identifier is intended to be part
/// of a `CamelCase` string for a class or an `ALL_CAPS_SNAKE_CASE` string for
/// a constant, since both conventions will produce the same output when given
/// a single input character. Therefore, this lint rule does not apply to cases
/// where the alias for the imported identifier consists of a single uppercase
/// character.
///
/// A common example of a single uppercase character being used for a class
/// name can be found in Django's `django.db.models.Q` class.
///
/// ## Options
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/
#[derive(ViolationMetadata)]
pub(crate) struct CamelcaseImportedAsConstant {
    name: String,
    asname: String,
}

impl Violation for CamelcaseImportedAsConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsConstant { name, asname } = self;
        format!("Camelcase `{name}` imported as constant `{asname}`")
    }
}

/// N814
pub(crate) fn camelcase_imported_as_constant(
    name: &str,
    asname: &str,
    alias: &Alias,
    stmt: &Stmt,
    ignore_names: &IgnoreNames,
) -> Option<Diagnostic> {
    // Single-character names are ambiguous. It could be a class or a constant.
    asname.chars().nth(1)?;

    if helpers::is_camelcase(name)
        && !str::is_cased_lowercase(asname)
        && str::is_cased_uppercase(asname)
        && !helpers::is_acronym(name, asname)
    {
        // Ignore any explicitly-allowed names.
        if ignore_names.matches(name) || ignore_names.matches(asname) {
            return None;
        }
        let mut diagnostic = Diagnostic::new(
            CamelcaseImportedAsConstant {
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
