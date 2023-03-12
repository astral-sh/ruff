use rustpython_parser::ast::{Excepthandler, Expr, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::except_range;
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for bare `except` catches in `try`-`except` statements.
///
/// ## Why is this bad?
/// A bare `except` catches `BaseException` which includes
/// `KeyboardInterrupt`, `SystemExit`, `Exception`, and others. Catching
/// `BaseException` can make it hard to interrupt the program (e.g., with
/// Ctrl-C) and disguise other problems.
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
/// ## References
/// - [PEP 8](https://www.python.org/dev/peps/pep-0008/#programming-recommendations)
/// - [Python: "Exception hierarchy"](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
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
pub fn bare_except(
    type_: Option<&Expr>,
    body: &[Stmt],
    handler: &Excepthandler,
    locator: &Locator,
) -> Option<Diagnostic> {
    if type_.is_none()
        && !body
            .iter()
            .any(|stmt| matches!(stmt.node, StmtKind::Raise { exc: None, .. }))
    {
        Some(Diagnostic::new(BareExcept, except_range(handler, locator)))
    } else {
        None
    }
}
