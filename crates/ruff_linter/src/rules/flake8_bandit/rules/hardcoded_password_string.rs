use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

use crate::rules::flake8_bandit::helpers::{matches_password_name, string_literal};

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
#[violation_metadata(stable_since = "v0.0.116", category = Category::Security)]
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

fn password_target(target: flat::View<'_, Expr>) -> Option<&str> {
    let target_name = match target.classify() {
        // variable = "s3cr3t"
        flat::ExprKind::Name(expr_name) => {
            let id = expr_name.id();
            id.as_str()
        }
        // d["password"] = "s3cr3t"
        flat::ExprKind::Subscript(expr_subscript) => {
            let slice = expr_subscript.slice();
            match slice.classify() {
                flat::ExprKind::StringLiteral(expr_string_literal) => {
                    let value = expr_string_literal.value();
                    value.to_str()
                }
                _ => return None,
            }
        }
        // obj.password = "s3cr3t"
        flat::ExprKind::Attribute(expr_attribute) => {
            let attr = expr_attribute.attr();
            attr.as_str()
        }
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
    compare: flat::View<'_, ast::ExprCompare>,
) {
    for comp in compare.comparators() {
        if string_literal(comp).is_none_or(str::is_empty) {
            continue;
        }
        let Some(name) = password_target(compare.first_operand()) else {
            continue;
        };
        checker.report_diagnostic(
            HardcodedPasswordString {
                name: name.to_string(),
            },
            comp.range(),
        );
    }
}

/// S105
pub(crate) fn assign_hardcoded_password_string<'a>(
    checker: &Checker,
    value: flat::View<'_, Expr>,
    targets: impl IntoIterator<Item = flat::View<'a, Expr>>,
) {
    if string_literal(value)
        .as_ref()
        .is_some_and(|string| !string.is_empty())
    {
        for target in targets {
            if let Some(name) = password_target(target) {
                checker.report_diagnostic(
                    HardcodedPasswordString {
                        name: name.to_string(),
                    },
                    value.range(),
                );
                return;
            }
        }
    }
}
