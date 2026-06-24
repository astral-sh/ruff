use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::except;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, Stmt};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for bare `except` catches in `try`-`except` statements.
///
/// ## Why is this bad?
/// A bare `except` catches `BaseException` which includes
/// `KeyboardInterrupt`, `SystemExit`, `Exception`, and others. Catching
/// `BaseException` can make it hard to interrupt the program (e.g., with
/// Ctrl-C) and can disguise other problems.
///
/// ## Example
/// ```python
/// try:
///     raise KeyboardInterrupt("You probably don't mean to break CTRL-C.")
/// except:
///     print("But a bare `except` will ignore keyboard interrupts.")
/// ```
///
/// Use instead:
/// ```python
/// try:
///     do_something_that_might_break()
/// except MoreSpecificException as e:
///     handle_error(e)
/// ```
///
/// If you actually need to catch an unknown error, use `Exception` which will
/// catch regular program errors but not important system exceptions.
///
/// ```python
/// def run_a_function(some_other_fn):
///     try:
///         some_other_fn()
///     except Exception as e:
///         print(f"How exceptional! {e}")
/// ```
///
/// ## References
/// - [Python documentation: Exception hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
/// - [Google Python Style Guide: "Exceptions"](https://google.github.io/styleguide/pyguide.html#24-exceptions)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.36")]
pub(crate) struct BareExcept;

impl Violation for BareExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not use bare `except`".to_string()
    }
}

/// E722
pub(crate) fn bare_except(
    checker: &Checker,
    type_: Option<&Expr>,
    body: &[Stmt],
    handler: &ExceptHandler,
) {
    if type_.is_none()
        && !body
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Raise(ast::StmtRaise { exc: None, .. })))
    {
        checker.report_diagnostic(BareExcept, except(handler, checker.locator().contents()));
    }
}
