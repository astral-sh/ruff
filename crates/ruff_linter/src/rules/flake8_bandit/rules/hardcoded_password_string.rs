use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::{matches_password_name, string_literal};

/// ## What it does
/// Checks for potential uses of hardcoded passwords in strings.
///
/// ## Why is this bad?
/// Including a hardcoded password in source code is a security risk, as an
/// attacker could discover the password and use it to gain unauthorized
/// access.
///
/// Instead, store passwords and other secrets in configuration files,
/// environment variables, or other sources that are excluded from version
/// control.
///
/// ## Example
/// ```python
/// SECRET_KEY = "hunter2"
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// SECRET_KEY = os.environ["SECRET_KEY"]
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-259](https://cwe.mitre.org/data/definitions/259.html)
#[derive(ViolationMetadata)]
pub(crate) struct HardcodedPasswordString {
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
    let target_name = match target {
        // variable = "s3cr3t"
        Expr::Name(ast::ExprName { id, .. }) => id.as_str(),
        // d["password"] = "s3cr3t"
        Expr::Subscript(ast::ExprSubscript { slice, .. }) => match slice.as_ref() {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.to_str(),
            _ => return None,
        },
        // obj.password = "s3cr3t"
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr,
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
    checker: &Checker,
    left: &Expr,
    comparators: &[Expr],
) {
    for comp in comparators {
        if string_literal(comp).is_none_or(str::is_empty) {
            continue;
        }
        let Some(name) = password_target(left) else {
            continue;
        };
        checker.report_diagnostic(Diagnostic::new(
            HardcodedPasswordString {
                name: name.to_string(),
            },
            comp.range(),
        ));
    }
}

/// S105
pub(crate) fn assign_hardcoded_password_string(checker: &Checker, value: &Expr, targets: &[Expr]) {
    if string_literal(value)
        .filter(|string| !string.is_empty())
        .is_some()
    {
        for target in targets {
            if let Some(name) = password_target(target) {
                checker.report_diagnostic(Diagnostic::new(
                    HardcodedPasswordString {
                        name: name.to_string(),
                    },
                    value.range(),
                ));
                return;
            }
        }
    }
}
