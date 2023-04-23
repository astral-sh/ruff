use rustc_hash::FxHashSet;
use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct BannedImportFrom(pub String);

impl Violation for BannedImportFrom {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedImportFrom(name) = self;
        format!("Members of `{name}` should not be imported explicitly")
    }
}

/// ICN003
pub fn check_banned_import_from(
    import_from: &Stmt,
    name: &str,
    banned_conventions: &FxHashSet<String>,
) -> Option<Diagnostic> {
    if banned_conventions.contains(name) {
        return Some(Diagnostic::new(
            BannedImportFrom(name.to_string()),
            Range::from(import_from),
        ));
    }
    None
}
