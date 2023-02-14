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

/// SLF001
pub fn private_member_access(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &expr.node {
        if !attr.ends_with("__") && (attr.starts_with('_') || attr.starts_with("__")) {
            if let ExprKind::Call { func, .. } = &value.node {
                let call_path = collect_call_path(func);
                if call_path.as_slice() == ["super"] {
                    return;
                }
            } else {
                let call_path = collect_call_path(value);
                if call_path.as_slice() == ["self"]
                    || call_path.as_slice() == ["cls"]
                    || call_path.as_slice() == ["mcs"]
                {
                    return;
                }
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
