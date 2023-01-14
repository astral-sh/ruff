use rustpython_ast::{ArgData, Arguments, Expr, Located};

use super::super::helpers::{matches_password_name, string_literal};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

fn check_password_kwarg(arg: &Located<ArgData>, default: &Expr) -> Option<Diagnostic> {
    let string = string_literal(default)?;
    let kwarg_name = &arg.node.arg;
    if !matches_password_name(kwarg_name) {
        return None;
    }
    Some(Diagnostic::new(
        violations::HardcodedPasswordDefault(string.to_string()),
        Range::from_located(default),
    ))
}

/// S107
pub fn hardcoded_password_default(arguments: &Arguments) -> Vec<Diagnostic> {
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
