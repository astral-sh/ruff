use rustc_hash::FxHashMap;
use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for imports that are typically imported using a common convention,
/// like `import pandas as pd`, and enforces that convention.
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for imports to make your code
/// more readable and idiomatic.
///
/// For example, `import pandas as pd` is a common
/// convention for importing the `pandas` library, and users typically expect
/// Pandas to be aliased as `pd`.
///
/// ## Example
/// ```python
/// import pandas
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
/// ```
#[violation]
pub struct UnconventionalImportAlias {
    pub name: String,
    pub asname: String,
}

impl Violation for UnconventionalImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnconventionalImportAlias { name, asname } = self;
        format!("`{name}` should be imported as `{asname}`")
    }
}

/// ICN001
pub fn conventional_import_alias(
    stmt: &Stmt,
    name: &str,
    asname: Option<&str>,
    conventions: &FxHashMap<String, String>,
) -> Option<Diagnostic> {
    if let Some(expected_alias) = conventions.get(name) {
        if asname != Some(expected_alias) {
            return Some(Diagnostic::new(
                UnconventionalImportAlias {
                    name: name.to_string(),
                    asname: expected_alias.to_string(),
                },
                stmt.range(),
            ));
        }
    }
    None
}
