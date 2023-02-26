use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, ExprKind, Located, StmtKind};

define_violation!(
    pub struct PreferOnlyEllipsis;
);
impl Violation for PreferOnlyEllipsis {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain only '...'")
    }
}

/// PYI010
pub fn prefer_only_ellipsis(checker: &mut Checker, body: &Vec<Located<StmtKind>>) {
    if body.len() != 1 {
        return;
    }
    if let StmtKind::Expr { value } = &body[0].node {
        if let ExprKind::Constant { value, .. } = &value.node {
            match value {
                Constant::Ellipsis | Constant::Str(_) => {}
                _ => {
                    checker.diagnostics.push(Diagnostic::new(
                        PreferOnlyEllipsis,
                        Range::from_located(&body[0]),
                    ));
                }
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                PreferOnlyEllipsis,
                Range::from_located(&body[0]),
            ));
        }
    } else {
        checker.diagnostics.push(Diagnostic::new(
            PreferOnlyEllipsis,
            Range::from_located(&body[0]),
        ));
    }
}
