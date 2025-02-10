use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, fix::edits::add_argument};

/// ## What it does
/// Checks for `warnings.warn` calls without an explicit `stacklevel` keyword
/// argument.
///
/// ## Why is this bad?
/// The `warnings.warn` method uses a `stacklevel` of 1 by default, which
/// will output a stack frame of the line on which the "warn" method
/// is called. Setting it to a higher number will output a stack frame
/// from higher up the stack.
///
/// It's recommended to use a `stacklevel` of 2 or higher, to give the caller
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
///
/// ## Fix safety
/// This rule's fix is marked as unsafe because it changes
/// the behavior of the code. Moreover, the fix will assign
/// a stacklevel of 2, while the user may wish to assign a
/// higher stacklevel to address the diagnostic.
///
/// ## References
/// - [Python documentation: `warnings.warn`](https://docs.python.org/3/library/warnings.html#warnings.warn)
#[derive(ViolationMetadata)]
pub(crate) struct NoExplicitStacklevel;

impl AlwaysFixableViolation for NoExplicitStacklevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        "No explicit `stacklevel` keyword argument found".to_string()
    }

    fn fix_title(&self) -> String {
        "Set `stacklevel=2`".to_string()
    }
}

/// B028
pub(crate) fn no_explicit_stacklevel(checker: &Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["warnings", "warn"]))
    {
        return;
    }

    if call
        .arguments
        .find_argument_value("stacklevel", 2)
        .is_some()
        || call
            .arguments
            .args
            .iter()
            .any(ruff_python_ast::Expr::is_starred_expr)
        || call
            .arguments
            .keywords
            .iter()
            .any(|keyword| keyword.arg.is_none())
    {
        return;
    }
    let mut diagnostic = Diagnostic::new(NoExplicitStacklevel, call.func.range());

    let edit = add_argument(
        "stacklevel=2",
        &call.arguments,
        checker.comment_ranges(),
        checker.locator().contents(),
    );

    diagnostic.set_fix(Fix::unsafe_edit(edit));

    checker.report_diagnostic(diagnostic);
}
