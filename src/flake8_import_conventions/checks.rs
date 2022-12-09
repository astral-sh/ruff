use rustc_hash::FxHashMap;
use rustpython_ast::Stmt;

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// ICN001
pub fn check_conventional_import(
    import_from: &Stmt,
    name: &str,
    asname: Option<&str>,
    conventions: &FxHashMap<String, String>,
) -> Option<Check> {
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
            return Some(Check::new(
                CheckKind::ImportAliasIsNotConventional(
                    name.to_string(),
                    expected_alias.to_string(),
                ),
                Range::from_located(import_from),
            ));
        }
    }
    None
}
