use ruff_python_ast::{Expr, Parameter, Parameters};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

use crate::rules::flake8_bandit::helpers::{matches_password_name, string_literal};

/// ## What it does
/// Checks for potential uses of hardcoded passwords in function argument
/// defaults.
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
///
/// ```python
/// def connect_to_server(password="hunter2"): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import os
///
///
/// def connect_to_server(password=os.environ["PASSWORD"]): ...
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-259](https://cwe.mitre.org/data/definitions/259.html)
#[derive(ViolationMetadata)]
pub(crate) struct HardcodedPasswordDefault {
    name: String,
}

impl Violation for HardcodedPasswordDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordDefault { name } = self;
        format!(
            "Possible hardcoded password assigned to function default: \"{}\"",
            name.escape_debug()
        )
    }
}

fn check_password_kwarg(checker: &Checker, parameter: &Parameter, default: &Expr) {
    if string_literal(default).is_none_or(str::is_empty) {
        return;
    }
    let kwarg_name = &parameter.name;
    if !matches_password_name(kwarg_name) {
        return;
    }
    checker.report_diagnostic(
        HardcodedPasswordDefault {
            name: kwarg_name.to_string(),
        },
        default.range(),
    );
}

/// S107
pub(crate) fn hardcoded_password_default(checker: &Checker, parameters: &Parameters) {
    for parameter in parameters.iter_non_variadic_params() {
        let Some(default) = parameter.default() else {
            continue;
        };
        check_password_kwarg(checker, &parameter.parameter, default);
    }
}
