use rustpython_ast::{ArgData, Arguments, Expr, Located};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_bandit::helpers::{matches_password_name, string_literal};

fn check_password_kwarg(arg: &Located<ArgData>, default: &Expr) -> Option<Check> {
    let string = string_literal(default)?;
    let kwarg_name = &arg.node.arg;
    if !matches_password_name(kwarg_name) {
        return None;
    }
    Some(Check::new(
        CheckKind::HardcodedPasswordDefault(string.to_string()),
        Range::from_located(default),
    ))
}

/// S107
pub fn hardcoded_password_default(arguments: &Arguments) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();

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
            if let Some(check) = check_password_kwarg(arg, default) {
                checks.push(check);
            }
        }
    }

    let defaults_start = arguments.kwonlyargs.len() - arguments.kw_defaults.len();
    for (i, kwarg) in arguments.kwonlyargs.iter().enumerate() {
        if let Some(i) = i.checked_sub(defaults_start) {
            let default = &arguments.kw_defaults[i];
            if let Some(check) = check_password_kwarg(kwarg, default) {
                checks.push(check);
            }
        }
    }

    checks
}
