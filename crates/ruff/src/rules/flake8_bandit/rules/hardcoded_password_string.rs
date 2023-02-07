use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use super::super::helpers::{matches_password_name, string_literal};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct HardcodedPasswordString {
        pub string: String,
    }
);
impl Violation for HardcodedPasswordString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordString { string } = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

fn is_password_target(target: &Expr) -> bool {
    let target_name = match &target.node {
        // variable = "s3cr3t"
        ExprKind::Name { id, .. } => id,
        // d["password"] = "s3cr3t"
        ExprKind::Subscript { slice, .. } => match &slice.node {
            ExprKind::Constant {
                value: Constant::Str(string),
                ..
            } => string,
            _ => return false,
        },
        // obj.password = "s3cr3t"
        ExprKind::Attribute { attr, .. } => attr,
        _ => return false,
    };

    matches_password_name(target_name)
}

/// S105
pub fn compare_to_hardcoded_password_string(left: &Expr, comparators: &[Expr]) -> Vec<Diagnostic> {
    comparators
        .iter()
        .filter_map(|comp| {
            let string = string_literal(comp).filter(|string| !string.is_empty())?;
            if !is_password_target(left) {
                return None;
            }
            Some(Diagnostic::new(
                HardcodedPasswordString {
                    string: string.to_string(),
                },
                Range::from_located(comp),
            ))
        })
        .collect()
}

/// S105
pub fn assign_hardcoded_password_string(value: &Expr, targets: &[Expr]) -> Option<Diagnostic> {
    if let Some(string) = string_literal(value).filter(|string| !string.is_empty()) {
        for target in targets {
            if is_password_target(target) {
                return Some(Diagnostic::new(
                    HardcodedPasswordString {
                        string: string.to_string(),
                    },
                    Range::from_located(value),
                ));
            }
        }
    }
    None
}
