use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct DfIsABadVariableName;
);
impl Violation for DfIsABadVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`df` is a bad variable name. Be kinder to your future self.")
    }
}

/// PD901
pub fn assignment_to_df(targets: &[Expr]) -> Option<Diagnostic> {
    if targets.len() != 1 {
        return None;
    }
    let target = &targets[0];
    let ExprKind::Name { id, .. } = &target.node else {
        return None;
    };
    if id != "df" {
        return None;
    }
    Some(Diagnostic::new(
        DfIsABadVariableName,
        Range::from_located(target),
    ))
}
