use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `print` statements that use the `>>` syntax.
///
/// ## Why is this bad?
/// In Python 2, the `print` statement can be used with the `>>` syntax to
/// print to a file-like object. This `print >> sys.stderr` syntax is
/// deprecated in Python 3.
///
/// Instead, use the `file` keyword argument to the `print` function, the
/// `sys.stderr.write` function, or the `logging` module.
///
/// ## Example
/// ```python
/// from __future__ import print_function
/// import sys
///
/// print >> sys.stderr, "Hello, world!"
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, world!", file=sys.stderr)
/// ```
///
/// Or:
/// ```python
/// import sys
///
/// sys.stderr.write("Hello, world!\n")
/// ```
///
/// Or:
/// ```python
/// import logging
///
/// logging.error("Hello, world!")
/// ```
///
/// ## References
/// - [Python documentation: `print`](https://docs.python.org/3/library/functions.html#print)
#[violation]
pub struct InvalidPrintSyntax;

impl Violation for InvalidPrintSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `>>` is invalid with `print` function")
    }
}

/// F633
pub(crate) fn invalid_print_syntax(checker: &mut Checker, left: &Expr) {
    let Expr::Name(ast::ExprName { id, .. }) = &left else {
        return;
    };
    if id != "print" {
        return;
    }
    if !checker.semantic().is_builtin("print") {
        return;
    };
    checker
        .diagnostics
        .push(Diagnostic::new(InvalidPrintSyntax, left.range()));
}
