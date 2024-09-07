use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, Stmt};
use ruff_python_stdlib::str::{self};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
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
/// Also note that import aliases following an import convention according to the
/// [`lint.flake8-import-conventions.aliases`] option are allowed.
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
///
/// ## Options
/// - `lint.flake8-import-conventions.aliases`
#[violation]
pub struct CamelcaseImportedAsAcronym {
    name: String,
    asname: String,
}

impl Violation for CamelcaseImportedAsAcronym {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsAcronym { name, asname } = self;
        format!("CamelCase `{name}` imported as acronym `{asname}`")
    }
}

/// N817
pub(crate) fn camelcase_imported_as_acronym(
    name: &str,
    asname: &str,
    alias: &Alias,
    stmt: &Stmt,
    checker: &Checker,
) -> Option<Diagnostic> {
    if helpers::is_camelcase(name)
        && !str::is_cased_lowercase(asname)
        && str::is_cased_uppercase(asname)
        && helpers::is_acronym(name, asname)
    {
        let ignore_names = &checker.settings.pep8_naming.ignore_names;

        // Ignore any explicitly-allowed names.
        if ignore_names.matches(name) || ignore_names.matches(asname) {
            return None;
        }

        // Ignore names that follow a community-agreed import convention.
        if is_ignored_because_of_import_convention(asname, stmt, alias, checker) {
            return None;
        }

        let mut diagnostic = Diagnostic::new(
            CamelcaseImportedAsAcronym {
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

fn is_ignored_because_of_import_convention(
    asname: &str,
    stmt: &Stmt,
    alias: &Alias,
    checker: &Checker,
) -> bool {
    let full_name = if let Some(import_from) = stmt.as_import_from_stmt() {
        // Never test relative imports for exclusion because we can't resolve the full-module name.
        let Some(module) = import_from.module.as_ref() else {
            return false;
        };

        if import_from.level != 0 {
            return false;
        }

        std::borrow::Cow::Owned(format!("{module}.{}", alias.name))
    } else {
        std::borrow::Cow::Borrowed(&*alias.name)
    };

    // Ignore names that follow a community-agreed import convention.
    checker
        .settings
        .flake8_import_conventions
        .aliases
        .get(&*full_name)
        .map(String::as_str)
        == Some(asname)
}
