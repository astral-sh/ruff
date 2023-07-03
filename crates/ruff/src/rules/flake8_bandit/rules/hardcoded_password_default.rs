use rustpython_parser::ast::{Arg, ArgWithDefault, Arguments, Expr, Ranged};

use crate::checkers::ast::Checker;
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
pub(crate) fn hardcoded_password_default(checker: &mut Checker, arguments: &Arguments) {
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
            checker.diagnostics.push(diagnostic);
        }
    }
}
