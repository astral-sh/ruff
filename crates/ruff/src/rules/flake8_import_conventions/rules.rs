use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    /// ### What it does
    /// Checks for imports that are typically imported using a common convention,
    /// like `import pandas as pd`, and enforces that convention.
    ///
    /// ### Why is this bad?
    /// Consistency is good. Use a common convention for imports to make your code
    /// more readable and idiomatic.
    ///
    /// For example, `import pandas as pd` is a common
    /// convention for importing the `pandas` library, and users typically expect
    /// Pandas to be aliased as `pd`.
    ///
    /// ### Example
    /// ```python
    /// import pandas
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// import pandas as pd
    /// ```
    pub struct UnconventionalImportAlias(pub String, pub String);
);
impl Violation for UnconventionalImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnconventionalImportAlias(name, asname) = self;
        format!("`{name}` should be imported as `{asname}`")
    }
}

/// ICN001
pub fn check_conventional_import(
    import_from: &Stmt,
    name: &str,
    asname: Option<&str>,
    conventions: &FxHashMap<String, String>,
) -> Option<Diagnostic> {
    let mut is_valid_import = true;
    if let Some(expected_alias) = conventions.get(name) {
        if !expected_alias.is_empty() {
            if let Some(alias) = asname {
                if expected_alias != alias {
                    is_valid_import = false;
                }
            } else {
                is_valid_import = false;
            }
        }
        if !is_valid_import {
            return Some(Diagnostic::new(
                UnconventionalImportAlias(name.to_string(), expected_alias.to_string()),
                Range::from_located(import_from),
            ));
        }
    }
    None
}
