use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

use ruff_macros::derive_message_formats;
use rustpython_ast::{Arguments, Stmt};

define_violation!(
    pub struct TooManyArgs {
        pub c_args: usize,
        pub max_args: usize,
    }
);

impl Violation for TooManyArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyArgs { c_args, max_args } = self;
        format!("Too many arguments ({c_args}/{max_args})")
    }
}

/// PLR0913
pub fn too_many_args(checker: &mut Checker, args: &Arguments, stmt: &Stmt) {
    let re = &checker.settings.pylint.ignored_argument_names;

    let filtered_args = args.args.iter().filter(|x| !re.0.is_match(&x.node.arg));
    let count = filtered_args.count();
    if count > checker.settings.pylint.max_args {
        checker.diagnostics.push(Diagnostic::new(
            TooManyArgs {
                c_args: args.args.len(),
                max_args: checker.settings.pylint.max_args,
            },
            Range::from_located(stmt),
        ));
    }
}
