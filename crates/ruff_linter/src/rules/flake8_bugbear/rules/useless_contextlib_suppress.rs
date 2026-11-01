use ruff_python_ast::ExprCallFields;
use ruff_python_ast::name::QualifiedName;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `contextlib.suppress` without arguments.
///
/// ## Why is this bad?
/// `contextlib.suppress` is a context manager that suppresses exceptions. It takes,
/// as arguments, the exceptions to suppress within the enclosed block. If no
/// exceptions are specified, then the context manager won't suppress any
/// exceptions, and is thus redundant.
///
/// Consider adding exceptions to the `contextlib.suppress` call, or removing the
/// context manager entirely.
///
/// ## Example
/// ```python
/// import contextlib
///
/// with contextlib.suppress():
///     foo()
/// ```
///
/// Use instead:
/// ```python
/// import contextlib
///
/// with contextlib.suppress(Exception):
///     foo()
/// ```
///
/// ## References
/// - [Python documentation: `contextlib.suppress`](https://docs.python.org/3/library/contextlib.html#contextlib.suppress)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.118", category = Category::Correctness)]
pub(crate) struct UselessContextlibSuppress;

impl Violation for UselessContextlibSuppress {
    #[derive_message_formats]
    fn message(&self) -> String {
        "No arguments passed to `contextlib.suppress`. No exceptions will be suppressed and \
            therefore this context manager is redundant"
            .to_string()
    }
}

/// B022
pub(crate) fn useless_contextlib_suppress(
    checker: &Checker,
    call: &ExprCallFields<'_>,
    qualified_name: &QualifiedName,
) {
    if call.arguments.args().is_empty()
        && matches!(qualified_name.segments(), ["contextlib", "suppress"])
    {
        checker.report_diagnostic(UselessContextlibSuppress, call.range);
    }
}
