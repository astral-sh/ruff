use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct PrivateMemberAccess {
        pub access: String,
    }
);
impl Violation for PrivateMemberAccess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PrivateMemberAccess { access } = self;
        format!("Private member accessed: `{access}`")
    }
}

const VALID_IDS: [&str; 3] = ["self", "cls", "mcs"];

/// SLF001
pub fn private_member_access(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &expr.node {
        if !attr.ends_with("__") && (attr.starts_with('_') || attr.starts_with("__")) {
            let id = match &value.node {
                ExprKind::Name { id, .. } => id,
                ExprKind::Attribute { attr, .. } => attr,
                _ => return,
            };

            if !VALID_IDS.contains(&id.as_str()) {
                checker.diagnostics.push(Diagnostic::new(
                    PrivateMemberAccess {
                        access: format!("{}.{}", id, attr),
                    },
                    Range::from_located(expr),
                ));
            }
        }
    }
}
