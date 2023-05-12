use rustpython_parser::ast::{self, Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use super::super::helpers::{matches_password_name, string_literal};

#[violation]
pub struct HardcodedPasswordString {
    name: String,
}

impl Violation for HardcodedPasswordString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordString { name } = self;
        format!(
            "Possible hardcoded password assigned to: \"{}\"",
            name.escape_debug()
        )
    }
}

fn password_target(target: &Expr) -> Option<&str> {
    let target_name = match &target.node {
        // variable = "s3cr3t"
        ExprKind::Name(ast::ExprName { id, .. }) => id.as_str(),
        // d["password"] = "s3cr3t"
        ExprKind::Subscript(ast::ExprSubscript { slice, .. }) => match &slice.node {
            ExprKind::Constant(ast::ExprConstant {
                value: Constant::Str(string),
                ..
            }) => string,
            _ => return None,
        },
        // obj.password = "s3cr3t"
        ExprKind::Attribute(ast::ExprAttribute { attr, .. }) => attr,
        _ => return None,
    };

    if matches_password_name(target_name) {
        Some(target_name)
    } else {
        None
    }
}

/// S105
pub(crate) fn compare_to_hardcoded_password_string(
    left: &Expr,
    comparators: &[Expr],
) -> Vec<Diagnostic> {
    comparators
        .iter()
        .filter_map(|comp| {
            string_literal(comp).filter(|string| !string.is_empty())?;
            let Some(name) = password_target(left) else {
                return None;
            };
            Some(Diagnostic::new(
                HardcodedPasswordString {
                    name: name.to_string(),
                },
                comp.range(),
            ))
        })
        .collect()
}

/// S105
pub(crate) fn assign_hardcoded_password_string(
    value: &Expr,
    targets: &[Expr],
) -> Option<Diagnostic> {
    if string_literal(value)
        .filter(|string| !string.is_empty())
        .is_some()
    {
        for target in targets {
            if let Some(name) = password_target(target) {
                return Some(Diagnostic::new(
                    HardcodedPasswordString {
                        name: name.to_string(),
                    },
                    value.range(),
                ));
            }
        }
    }
    None
}
