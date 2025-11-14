use rustc_hash::FxHashMap;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::preview::is_import_conventions_preview_enabled;
use crate::rules::flake8_import_conventions::settings::{BannedAliases, preview_banned_aliases};

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
///
/// ## Options
/// - `lint.flake8-import-conventions.banned-aliases`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.262")]
pub(crate) struct BannedImportAlias {
    name: String,
    asname: String,
}

impl Violation for BannedImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedImportAlias { name, asname } = self;
        format!("`{name}` should not be imported as `{asname}`")
    }
}

/// ICN002
pub(crate) fn banned_import_alias(
    checker: &Checker,
    stmt: &Stmt,
    name: &str,
    asname: &str,
    banned_conventions: &FxHashMap<String, BannedAliases>,
) {
    // Merge preview banned aliases if preview mode is enabled
    let banned_aliases = if is_import_conventions_preview_enabled(checker.settings()) {
        banned_conventions.get(name).cloned().or_else(|| {
            let preview_banned = preview_banned_aliases();
            preview_banned.get(name).cloned()
        })
    } else {
        banned_conventions.get(name).cloned()
    };

    if let Some(banned_aliases) = banned_aliases.as_ref() {
        if banned_aliases
            .iter()
            .any(|banned_alias| banned_alias == asname)
        {
            checker.report_diagnostic(
                BannedImportAlias {
                    name: name.to_string(),
                    asname: asname.to_string(),
                },
                stmt.range(),
            );
        }
    }
}
