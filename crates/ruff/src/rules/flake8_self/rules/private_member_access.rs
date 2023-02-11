use rustpython_parser::ast::{Expr, ExprKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers::collect_call_path;
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
            let call_path = collect_call_path(value);
            if VALID_IDS.iter().any(|id| call_path.as_slice() == [*id]) {
                return;
            }

            checker.diagnostics.push(Diagnostic::new(
                PrivateMemberAccess {
                    access: attr.to_string(),
                },
                Range::from_located(expr),
            ));
        }
    }
}
