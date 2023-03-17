use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `warnings.warn` calls without an explicit `stacklevel` keyword
/// argument.
///
/// ## Why is this bad?
/// The `warnings.warn` method uses a `stacklevel` of 1 by default, which
/// limits the rendered stack trace to that of the line on which the
/// `warn` method is called.
///
/// It's recommended to use a `stacklevel` of 2 or higher, give the caller
/// more context about the warning.
///
/// ## Example
/// ```python
/// warnings.warn("This is a warning")
/// ```
///
/// Use instead:
/// ```python
/// warnings.warn("This is a warning", stacklevel=2)
/// ```
#[violation]
pub struct NoExplicitStacklevel;

impl Violation for NoExplicitStacklevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No explicit `stacklevel` keyword argument found")
    }
}

/// B028
pub fn no_explicit_stacklevel(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["warnings", "warn"]
        })
    {
        return;
    }

    if SimpleCallArgs::new(args, keywords)
        .keyword_argument("stacklevel")
        .is_some()
    {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(NoExplicitStacklevel, Range::from(func)));
}
