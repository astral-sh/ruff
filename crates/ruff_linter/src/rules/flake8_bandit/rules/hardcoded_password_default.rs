use ruff_python_ast::{Expr, Parameter, ParameterWithDefault, Parameters};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::{matches_password_name, string_literal};

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
#[violation]
pub struct HardcodedPasswordDefault {
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

fn check_password_kwarg(parameter: &Parameter, default: &Expr) -> Option<Diagnostic> {
    string_literal(default).filter(|string| !string.is_empty())?;
    let kwarg_name = &parameter.name;
    if !matches_password_name(kwarg_name) {
        return None;
    }
    Some(Diagnostic::new(
        HardcodedPasswordDefault {
            name: kwarg_name.to_string(),
        },
        default.range(),
    ))
}

/// S107
pub(crate) fn hardcoded_password_default(checker: &mut Checker, parameters: &Parameters) {
    for ParameterWithDefault {
        parameter,
        default,
        range: _,
    } in parameters.iter_non_variadic_params()
    {
        let Some(default) = default else {
            continue;
        };
        if let Some(diagnostic) = check_password_kwarg(parameter, default) {
            checker.diagnostics.push(diagnostic);
        }
    }
}
