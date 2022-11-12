use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_bandit::helpers::{matches_password_name, string_literal};

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
pub fn compare_to_hardcoded_password_string(left: &Expr, comparators: &[Expr]) -> Vec<Check> {
    comparators
        .iter()
        .filter_map(|comp| {
            if let Some(string) = string_literal(comp) {
                if is_password_target(left) {
                    return Some(Check::new(
                        CheckKind::HardcodedPasswordString(string.to_string()),
                        Range::from_located(comp),
                    ));
                }
            }
            None
        })
        .collect()
}

/// S105
pub fn assign_hardcoded_password_string(value: &Expr, targets: &Vec<Expr>) -> Option<Check> {
    if let Some(string) = string_literal(value) {
        for target in targets {
            if is_password_target(target) {
                return Some(Check::new(
                    CheckKind::HardcodedPasswordString(string.to_string()),
                    Range::from_located(value),
                ));
            }
        }
    }
    None
}
