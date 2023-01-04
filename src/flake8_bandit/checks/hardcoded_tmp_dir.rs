use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};

/// S108
pub fn hardcoded_tmp_dir<'a>(
    expr: &Expr,
    value: &str,
    prefixes: &mut impl Iterator<Item = &'a String>,
) -> Option<Check> {
    if prefixes.any(|prefix| value.starts_with(prefix)) {
        Some(Check::new(
            CheckKind::HardcodedTempFile(value.to_string()),
            Range::from_located(expr),
        ))
    } else {
        None
    }
}
