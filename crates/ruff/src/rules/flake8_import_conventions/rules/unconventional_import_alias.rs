use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Scope;

use crate::checkers::ast::Checker;

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
    name: String,
    asname: String,
}

impl Violation for UnconventionalImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnconventionalImportAlias { name, asname } = self;
        format!("`{name}` should be imported as `{asname}`")
    }
}

/// ICN001
pub(crate) fn unconventional_import_alias(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
    conventions: &FxHashMap<String, String>,
) -> Option<Diagnostic> {
    for (name, binding_id) in scope.all_bindings() {
        let binding = checker.semantic().binding(binding_id);

        let Some(qualified_name) = binding.qualified_name() else {
            continue;
        };

        let Some(expected_alias) = conventions.get(qualified_name) else {
            continue;
        };

        if binding.is_alias() && name == expected_alias {
            continue;
        }

        diagnostics.push(Diagnostic::new(
            UnconventionalImportAlias {
                name: qualified_name.to_string(),
                asname: expected_alias.to_string(),
            },
            binding.range,
        ));
    }
    None
}
