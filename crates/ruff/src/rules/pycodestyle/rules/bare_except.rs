use ruff_python_ast::{self as ast, ExceptHandler, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::except;
use ruff_source_file::Locator;

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
#[violation]
pub struct BareExcept;

impl Violation for BareExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use bare `except`")
    }
}

/// E722
pub(crate) fn bare_except(
    type_: Option<&Expr>,
    body: &[Stmt],
    handler: &ExceptHandler,
    locator: &Locator,
) -> Option<Diagnostic> {
    if type_.is_none()
        && !body
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Raise(ast::StmtRaise { exc: None, .. })))
    {
        Some(Diagnostic::new(
            BareExcept,
            except(handler, locator.contents()),
        ))
    } else {
        None
    }
}
