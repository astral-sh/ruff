use rustpython_parser::ast::{ArgWithDefault, Arguments, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};

use crate::checkers::ast::Checker;

#[violation]
pub struct MutableArgumentDefault;

impl Violation for MutableArgumentDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for argument defaults")
    }
}

/// B006
pub(crate) fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
    // Scan in reverse order to right-align zip().
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
        let Some(default)= default else {
            continue;
        };

        if is_mutable_expr(default, checker.semantic())
            && !def.annotation.as_ref().map_or(false, |expr| {
                is_immutable_annotation(expr, checker.semantic())
            })
        {
            checker
                .diagnostics
                .push(Diagnostic::new(MutableArgumentDefault, default.range()));
        }
    }
}
