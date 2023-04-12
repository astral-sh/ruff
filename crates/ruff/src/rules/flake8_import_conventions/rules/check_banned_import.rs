use rustc_hash::FxHashMap;
use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

/// ## What it does
/// Checks for imports that use non-standard naming conventions, like
/// `import tensorflow.keras.backend as K`.
///
/// ## Why is this bad?
/// Consistency is good. Avoid using a non-standard naming convention for
/// imports, and, in particular, choosing import aliases that violate PEP 8.
///
/// For example, aliasing via `import tensorflow.keras.backend as K` violates
/// the guidance of PEP 8, and is thus avoided in some projects.
///
/// ## Example
/// ```python
/// import tensorflow.keras.backend as K
/// ```
///
/// Use instead:
/// ```python
/// import tensorflow as tf
///
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
    asname: &str,
    banned_conventions: &FxHashMap<String, Vec<String>>,
) -> Option<Diagnostic> {
    if let Some(banned_aliases) = banned_conventions.get(name) {
        if banned_aliases
            .iter()
            .any(|banned_alias| banned_alias == asname)
        {
            return Some(Diagnostic::new(
                BannedImportAlias(name.to_string(), asname.to_string()),
                Range::from(import_from),
            ));
        }
    }
    None
}
