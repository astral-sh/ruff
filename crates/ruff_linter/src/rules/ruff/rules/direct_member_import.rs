use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for imports of specific members (functions, classes, etc.) from modules
/// using the `from module import member` syntax.
///
/// ## Why is this bad?
/// Following [Google's Python Style Guide section 2.2](https://google.github.io/styleguide/pyguide.html#22-imports),
/// imports should be for packages and modules only, not for individual types,
/// classes, or functions.
///
/// The namespace management convention is simple. The source of each identifier
/// is indicated in a consistent way; `x.Obj` says that object `Obj` is defined
/// in module `x`.
///
/// For example, `import os` followed by `os.path.join()` is clearer than
/// `from os import path` followed by `path.join()`, as the fully qualified
/// name makes the origin explicit.
///
/// ## Exceptions
/// Imports from the following modules are exempt from this rule, per
/// [Google's Style Guide](https://google.github.io/styleguide/pyguide.html#2241-exemptions):
/// - `typing`: Symbols used to support static analysis and type checking
/// - `typing_extensions`: Symbols used to support static analysis and type checking
/// - `collections.abc`: Symbols used to support static analysis and type checking
/// - `six.moves`: Redirects for Python 2/3 compatibility
///
/// ## Example
/// ```python
/// from os import path
/// from pathlib import Path
/// from json import loads, dumps
/// ```
///
/// Use instead:
/// ```python
/// import os
/// import pathlib
/// import json
///
/// # Then use as: os.path, pathlib.Path, json.loads, json.dumps
/// ```
///
/// Or, for modules within a package (following Google's style):
/// ```python
/// # If importing module `echo` from package `sound.effects`
/// from sound.effects import echo
/// echo.EchoFilter(input, output, delay=0.7, atten=4)
/// ```
///
/// ## References
/// - [Google Python Style Guide: 2.2 Imports](https://google.github.io/styleguide/pyguide.html#22-imports)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.0")]
pub(crate) struct DirectMemberImport {
    module: String,
    member: String,
}

impl Violation for DirectMemberImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DirectMemberImport { module, member } = self;
        format!("Prefer importing the module `{module}` instead of member `{member}`")
    }
}

/// Check if a module is exempt from the rule (e.g., typing modules)
///
/// Exemptions per Google Python Style Guide section 2.2.4.1:
/// - `typing`, `typing_extensions`, `collections.abc`: static analysis and type checking
/// - `six.moves`: Python 2/3 compatibility redirects
fn is_exempt_module(module: Option<&str>) -> bool {
    matches!(
        module,
        Some("typing" | "typing_extensions" | "collections.abc" | "six.moves")
    )
}

/// RUF066
pub(crate) fn direct_member_import(checker: &mut Checker, import_from: &ast::StmtImportFrom) {
    let ast::StmtImportFrom {
        module,
        names,
        level,
        ..
    } = import_from;

    // Skip relative imports (e.g., `from . import foo`)
    if *level > 0 {
        return;
    }

    // Skip if module is None (shouldn't happen with level=0, but be safe)
    let Some(module_name) = module.as_deref() else {
        return;
    };

    // Skip exempt modules like typing
    if is_exempt_module(Some(module_name)) {
        return;
    }

    // Check each imported name
    for alias in names {
        // Skip wildcard imports (handled by other rules)
        if alias.name.as_str() == "*" {
            continue;
        }

        // Report the violation
        checker.report_diagnostic(
            DirectMemberImport {
                module: module_name.to_string(),
                member: alias.name.to_string(),
            },
            alias.range(),
        );
    }
}
