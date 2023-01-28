use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;
use rustpython_ast::{Arguments, Stmt};

/// PLR0913
pub fn too_many_args(checker: &mut Checker, args: &Arguments, stmt: &Stmt) {
    if args.args.len() > checker.settings.pylint.max_args {
        checker.diagnostics.push(Diagnostic::new(
            violations::TooManyArgs {
                c_args: args.args.len(),
                max_args: checker.settings.pylint.max_args,
            },
            Range::from_located(stmt),
        ))
    }
}
