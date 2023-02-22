use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct NoExplicitStacklevel;

impl Violation for NoExplicitStacklevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "No explicit stacklevel keyword argument found. The warn method from the warnings module uses a stacklevel of 1 by default. This will only show a stack trace for the line on which the warn method is called."
            )
    }
}

/// B028
pub fn no_explicit_stacklevel(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let is_warn_call = checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["warnings", "warn"]
        });

    let call_args = SimpleCallArgs::new(args, keywords);
    if is_warn_call && call_args.keyword_argument("stacklevel").is_none() {
        checker
            .diagnostics
            .push(Diagnostic::new(NoExplicitStacklevel, Range::from(func)));
    }
}
