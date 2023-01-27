use ruff_macros::derive_message_formats;
use rustc_hash::FxHashMap;
use rustpython_ast::Stmt;

use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct ImportAliasIsNotConventional(pub String, pub String);
);
impl Violation for ImportAliasIsNotConventional {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportAliasIsNotConventional(name, asname) = self;
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
                ImportAliasIsNotConventional(name.to_string(), expected_alias.to_string()),
                Range::from_located(import_from),
            ));
        }
    }
    None
}
