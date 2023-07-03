use rustpython_parser::ast::{Arg, ArgWithDefault, Arguments, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use super::super::helpers::{matches_password_name, string_literal};

/// ## What it does
/// Checks for hardcoded password argument defaults.
///
/// ## Why is this bad?
/// Hardcoded passwords are a security risk as they can be easily discovered
/// by attackers and used to gain unauthorized access. As they are hardcoded,
/// this vulnerability cannot be easily fixed by the end user without changing
/// the source code.
///
/// Instead of hardcoding passwords, consider storing them in configuration
/// files or other stores that are not committed to version control.
///
/// ## Example
/// ```python
/// def connect_to_server(password="hunter2"):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import os
///
///
/// def connect_to_server(password=os.environ["PASSWORD"]):
///     ...
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

fn check_password_kwarg(arg: &Arg, default: &Expr) -> Option<Diagnostic> {
    string_literal(default).filter(|string| !string.is_empty())?;
    let kwarg_name = &arg.arg;
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
pub(crate) fn hardcoded_password_default(arguments: &Arguments) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    for ArgWithDefault {
        def,
        default,
        range: _,
    } in arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .chain(&arguments.kwonlyargs)
    {
        let Some(default) = default else {
            continue;
        };
        if let Some(diagnostic) = check_password_kwarg(def, default) {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}
