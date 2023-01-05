use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};

/// S108
pub fn hardcoded_tmp_dir(expr: &Expr, value: &str, prefixes: &[String]) -> Option<Check> {
    if prefixes.iter().any(|prefix| value.starts_with(prefix)) {
        Some(Check::new(
            CheckKind::HardcodedTempFile(value.to_string()),
            Range::from_located(expr),
        ))
    } else {
        None
    }
}
