use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `print` statements that use the `>>` syntax.
///
/// ## Why is this bad?
/// In Python 2, the `print` statement can be used with the `>>` syntax to
/// print to a file-like object. This `print >> sys.stderr` syntax no
/// longer exists in Python 3, where `print` is only a function, not a
/// statement.
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
#[derive(ViolationMetadata)]
pub(crate) struct InvalidPrintSyntax;

impl Violation for InvalidPrintSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `>>` is invalid with `print` function".to_string()
    }
}

/// F633
pub(crate) fn invalid_print_syntax(checker: &Checker, left: &Expr) {
    if checker.semantic().match_builtin_expr(left, "print") {
        checker.report_diagnostic(Diagnostic::new(InvalidPrintSyntax, left.range()));
    }
}
