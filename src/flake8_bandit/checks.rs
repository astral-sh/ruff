use rustpython_ast::{ArgData, Arguments, Constant, Expr, ExprKind, KeywordData, Located};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

const PASSWORD_NAMES: [&str; 7] = [
    "password", "pass", "passwd", "pwd", "secret", "token", "secrete",
];

fn string_literal(expr: &Expr) -> Option<&str> {
    match &expr.node {
        ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } => Some(string),
        _ => None,
    }
}

// Maybe use regex for this?
fn matches_password_name(string: &str) -> bool {
    PASSWORD_NAMES
        .iter()
        .any(|name| string.to_lowercase().contains(name))
}

/// S102
pub fn exec_used(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "exec" {
            checker.add_check(Check::new(CheckKind::ExecUsed, Range::from_located(expr)));
        }
    }
}

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

fn check_password_kwarg(arg: &Located<ArgData>, default: &Expr) -> Option<Check> {
    if let Some(string) = string_literal(default) {
        let kwarg_name = &arg.node.arg;
        if matches_password_name(kwarg_name) {
            return Some(Check::new(
                CheckKind::HardcodedPasswordDefault(string.to_string()),
                Range::from_located(default),
            ));
        }
    }
    None
}

/// S104
pub fn hardcoded_bind_all_interfaces(value: &str, range: &Range) -> Option<Check> {
    if value == "0.0.0.0" {
        return Some(Check::new(CheckKind::HardcodedBindAllInterfaces, *range));
    } else {
        None
    }
}

/// S105
pub fn compare_to_hardcoded_password_string(left: &Expr, comparators: &[Expr]) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();

    comparators.iter().for_each(|comp| {
        if let Some(string) = string_literal(comp) {
            if is_password_target(left) {
                checks.push(Check::new(
                    CheckKind::HardcodedPasswordString(string.to_string()),
                    Range::from_located(comp),
                ));
            }
        }
    });
    checks
}

/// S105
pub fn assign_hardcoded_password_string(value: &Expr, targets: &Vec<Expr>) -> Option<Check> {
    if let Some(string) = string_literal(value) {
        for target in targets {
            if is_password_target(target) {
                return Some(Check::new(
                    CheckKind::HardcodedPasswordString(string.to_string()),
                    Range::from_located(value),
                ));
            }
        }
    }
    None
}

/// S106
pub fn hardcoded_password_funcarg(keywords: &Vec<Located<KeywordData>>) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();

    for keyword in keywords {
        if let Some(string) = string_literal(&keyword.node.value) {
            if let Some(arg) = &keyword.node.arg {
                if matches_password_name(arg) {
                    checks.push(Check::new(
                        CheckKind::HardcodedPasswordFuncArg(string.to_string()),
                        Range::from_located(&keyword),
                    ));
                }
            }
        }
    }
    checks
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
