use rustc_hash::FxHashMap;
use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

/// ## What it does
/// Checks for imports that should not be using a non-standard convention,
/// like `import tensorflow.keras.backend as K`, and suggest avoiding such practice.
///
/// ## Why is this bad?
/// Consistency is good. Avoid using a non-standard convention for imports violating
/// PEP 8 principle to make your code more readable idiomatic.
///
/// For example, `import tensorflow.keras.backend as K` is an example of violating
/// PEP 8 principle, and users should typically avoid such imports in large codebases.
///
/// ## Example
/// ```python
/// import tensorflow.keras.backend as K
/// ```
///
/// Use instead, for example,:
/// ```python
/// import tensorflow as tf
/// ...
/// tf.keras.backend
/// ```
#[violation]
pub struct BannedImportAlias(pub String, pub String);

impl Violation for BannedImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedImportAlias(name, asname) = self;
        format!("`{name}` should not be imported as `{asname}`")
    }
}

/// ICN002
pub fn check_banned_import(
    import_from: &Stmt,
    name: &str,
    asname: Option<&str>,
    banned_conventions: &FxHashMap<String, Vec<String>>,
) -> Option<Diagnostic> {
    if let Some(banned_aliases) = banned_conventions.get(name) {
        let mut is_valid_import = true;
        for banned_alias in banned_aliases {
            if !banned_alias.is_empty() {
                if let Some(alias) = asname {
                    if banned_alias == alias {
                        is_valid_import = false;
                    }
                } else {
                    is_valid_import = false;
                }
                break;
            }
        }
        if !is_valid_import {
            return Some(Diagnostic::new(
                BannedImportAlias(name.to_string(), banned_aliases.join(", ")),
                Range::from(import_from),
            ));
        }
    }
    None
}
