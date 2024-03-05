use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

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
pub(crate) fn no_explicit_stacklevel(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["warnings", "warn"]))
    {
        return;
    }

    if call.arguments.find_keyword("stacklevel").is_some() {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(NoExplicitStacklevel, call.func.range()));
}
