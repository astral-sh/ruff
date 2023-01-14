use rustpython_ast::{Constant, Expr, ExprKind};

use super::super::helpers::{matches_password_name, string_literal};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

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
            let string = string_literal(comp)?;
            if !is_password_target(left) {
                return None;
            }
            Some(Diagnostic::new(
                violations::HardcodedPasswordString(string.to_string()),
                Range::from_located(comp),
            ))
        })
        .collect()
}

/// S105
pub fn assign_hardcoded_password_string(value: &Expr, targets: &[Expr]) -> Option<Diagnostic> {
    if let Some(string) = string_literal(value) {
        for target in targets {
            if is_password_target(target) {
                return Some(Diagnostic::new(
                    violations::HardcodedPasswordString(string.to_string()),
                    Range::from_located(value),
                ));
            }
        }
    }
    None
}
