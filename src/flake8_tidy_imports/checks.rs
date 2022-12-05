use rustpython_ast::Stmt;

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_tidy_imports::settings::Strictness;

pub fn banned_relative_import(
    stmt: &Stmt,
    level: Option<&usize>,
    strictness: &Strictness,
) -> Option<Check> {
    let strictness_level = match strictness {
        Strictness::All => 0,
        Strictness::Parents => 1,
    };
    if level? > &strictness_level {
        Some(Check::new(
            CheckKind::BannedRelativeImport(strictness.clone()),
            Range::from_located(stmt),
        ))
    } else {
        None
    }
}
