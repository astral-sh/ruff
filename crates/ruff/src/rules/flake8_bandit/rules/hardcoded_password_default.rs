use rustpython_parser::ast::{Arg, Arguments, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use super::super::helpers::{matches_password_name, string_literal};

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

    let defaults_start =
        arguments.posonlyargs.len() + arguments.args.len() - arguments.defaults.len();
    for (i, arg) in arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .enumerate()
    {
        if let Some(i) = i.checked_sub(defaults_start) {
            let default = &arguments.defaults[i];
            if let Some(diagnostic) = check_password_kwarg(arg, default) {
                diagnostics.push(diagnostic);
            }
        }
    }

    let defaults_start = arguments.kwonlyargs.len() - arguments.kw_defaults.len();
    for (i, kwarg) in arguments.kwonlyargs.iter().enumerate() {
        if let Some(i) = i.checked_sub(defaults_start) {
            let default = &arguments.kw_defaults[i];
            if let Some(diagnostic) = check_password_kwarg(kwarg, default) {
                diagnostics.push(diagnostic);
            }
        }
    }

    diagnostics
}
