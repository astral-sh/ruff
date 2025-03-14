use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Alias, Stmt};
use ruff_python_stdlib::str;
use ruff_text_size::Ranged;

use crate::rules::pep8_naming::{helpers, settings::IgnoreNames};

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
/// ## Note
/// Identifiers consisting of a single uppercase character are ambiguous under
/// the rules of [PEP 8], which specifies `CamelCase` for classes and
/// `ALL_CAPS_SNAKE_CASE` for constants. Without a second character, it is not
/// possible to reliably guess whether the identifier is intended to be part
/// of a `CamelCase` string for a class or an `ALL_CAPS_SNAKE_CASE` string for
/// a constant, since both conventions will produce the same output when given
/// a single input character. Therefore, this lint rule does not apply to cases
/// where the imported identifier consists of a single uppercase character.
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
pub(crate) struct ConstantImportedAsNonConstant {
    name: String,
    asname: String,
}

impl Violation for ConstantImportedAsNonConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConstantImportedAsNonConstant { name, asname } = self;
        format!("Constant `{name}` imported as non-constant `{asname}`")
    }
}

/// N811
pub(crate) fn constant_imported_as_non_constant(
    name: &str,
    asname: &str,
    alias: &Alias,
    stmt: &Stmt,
    ignore_names: &IgnoreNames,
) -> Option<Diagnostic> {
    if str::is_cased_uppercase(name)
        && !(str::is_cased_uppercase(asname)
            // Single-character names are ambiguous.
            // It could be a class or a constant, so allow it to be imported
            // as `SCREAMING_SNAKE_CASE` *or* `CamelCase`.
            || (name.chars().nth(1).is_none() && helpers::is_camelcase(asname)))
    {
        // Ignore any explicitly-allowed names.
        if ignore_names.matches(name) || ignore_names.matches(asname) {
            return None;
        }
        let mut diagnostic = Diagnostic::new(
            ConstantImportedAsNonConstant {
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
