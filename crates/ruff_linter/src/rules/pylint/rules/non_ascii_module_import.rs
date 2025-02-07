use ruff_python_ast::Alias;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of non-ASCII characters in import statements.
///
/// ## Why is this bad?
/// The use of non-ASCII characters in import statements can cause confusion
/// and compatibility issues (see: [PEP 672]).
///
/// ## Example
/// ```python
/// import bár
/// ```
///
/// Use instead:
/// ```python
/// import bar
/// ```
///
/// If the module is third-party, use an ASCII-only alias:
/// ```python
/// import bár as bar
/// ```
///
/// [PEP 672]: https://peps.python.org/pep-0672/
#[derive(ViolationMetadata)]
pub(crate) struct NonAsciiImportName {
    name: String,
    kind: Kind,
}

impl Violation for NonAsciiImportName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name, kind } = self;
        match kind {
            Kind::Aliased => {
                format!("Module alias `{name}` contains a non-ASCII character")
            }
            Kind::Unaliased => {
                format!("Module name `{name}` contains a non-ASCII character")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use an ASCII-only alias".to_string())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Kind {
    /// The import uses a non-ASCII alias (e.g., `import foo as bár`).
    Aliased,
    /// The imported module is non-ASCII, and could be given an ASCII alias (e.g., `import bár`).
    Unaliased,
}

/// PLC2403
pub(crate) fn non_ascii_module_import(checker: &Checker, alias: &Alias) {
    if let Some(asname) = &alias.asname {
        if asname.as_str().is_ascii() {
            return;
        }

        checker.report_diagnostic(Diagnostic::new(
            NonAsciiImportName {
                name: asname.to_string(),
                kind: Kind::Aliased,
            },
            asname.range(),
        ));
    } else {
        if alias.name.as_str().is_ascii() {
            return;
        }

        checker.report_diagnostic(Diagnostic::new(
            NonAsciiImportName {
                name: alias.name.to_string(),
                kind: Kind::Unaliased,
            },
            alias.name.range(),
        ));
    }
}
