use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};
use crate::violations;

/// S108
pub fn hardcoded_tmp_directory(expr: &Expr, value: &str, prefixes: &[String]) -> Option<Check> {
    if prefixes.iter().any(|prefix| value.starts_with(prefix)) {
        Some(Check::new(
            violations::HardcodedTempFile(value.to_string()),
            Range::from_located(expr),
        ))
    } else {
        None
    }
}
