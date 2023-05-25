use crate::checkers::ast::Checker;
use rustpython_parser::ast::StmtWhile;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for `while` loops.
///
/// ## Why is this bad?
/// `while` loops can hard to read and understand. They are also prone to
/// infinite loops.
///
/// Often, a `while` loop can be rewritten as an alternative construct, such as
/// a `for` loop or context manager. These are often more explicit, less error
/// prone, and more easily optimized by the interpreter.
///
/// Exceptions to this are loops that are intended to run indefinitely, such as
/// event loops and listeners.
///
/// ## Example
/// ```python
/// i = n
/// while i > 0:
///     print(i)
///     i -= 1
/// ```
///
/// Use instead:
/// ```python
/// for i in range(n, 0, -1):
///     print(i)
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/compound_stmts.html#the-while-statement)
/// - [Python documentation](https://docs.python.org/3/reference/compound_stmts.html#the-for-statement)
#[violation]
pub struct WhileLoop;

impl Violation for WhileLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Used `while` loop")
    }
}

/// W0149
pub(crate) fn while_loop(checker: &mut Checker, stmt_while: &StmtWhile) {
    checker
        .diagnostics
        .push(Diagnostic::new(WhileLoop, stmt_while.range));
}
