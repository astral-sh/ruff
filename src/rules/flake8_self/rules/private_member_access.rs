use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use rustpython_ast::{Expr, ExprKind};

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
            let ExprKind::Name { id, .. } = &value.node else {
                return;
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
