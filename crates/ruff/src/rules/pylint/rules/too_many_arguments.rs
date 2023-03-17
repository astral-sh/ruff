use rustpython_parser::ast::{Arguments, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;

use crate::checkers::ast::Checker;

#[violation]
pub struct TooManyArguments {
    pub c_args: usize,
    pub max_args: usize,
}

impl Violation for TooManyArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyArguments { c_args, max_args } = self;
        format!("Too many arguments to function call ({c_args} > {max_args})")
    }
}

/// PLR0913
pub fn too_many_arguments(checker: &mut Checker, args: &Arguments, stmt: &Stmt) {
    let num_args = args
        .args
        .iter()
        .filter(|arg| !checker.settings.dummy_variable_rgx.is_match(&arg.node.arg))
        .count();
    if num_args > checker.settings.pylint.max_args {
        checker.diagnostics.push(Diagnostic::new(
            TooManyArguments {
                c_args: num_args,
                max_args: checker.settings.pylint.max_args,
            },
            identifier_range(stmt, checker.locator),
        ));
    }
}
