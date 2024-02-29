use ast::{ExprCall, StmtRaise};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// - [Python exception hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
const PY_BUILTIN_EXCEPTIONS: [&str; 21] = [
    "SystemExit",
    "Exception",
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BufferError",
    "EOFError",
    "ImportError",
    "LookupError",
    "IndexError",
    "KeyError",
    "MemoryError",
    "NameError",
    "ReferenceError",
    "RuntimeError",
    "NotImplementedError",
    "StopIteration",
    "SyntaxError",
    "SystemError",
    "TypeError",
    "ValueError",
];

/// ## What it does
/// Checks for a missing `raise` statement for an exception. It's unnecessary to
/// use an exception without raising it. This rule checks for the absence of a
/// `raise` statement for an exception.
///
/// ## Why is this bad?
/// It's unnecessary to use an exception without raising it. This can lead to
/// confusion and unexpected behavior. It's better to raise the exception to
/// indicate that an error has occurred.
///
/// ## Example
/// ```python
/// Exception("exception should be raised")
/// ```
///
/// Use instead:
/// ```python
/// raise Exception("exception should be raised")
/// ```
#[violation]
pub struct UselessExceptionStatement;

impl Violation for UselessExceptionStatement {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing `raise` statement for exception; add `raise` statement to exception")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Add `raise` statement to exception"))
    }
}

/// PLW0133
pub(crate) fn useless_exception_statement(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ExprCall { func, .. }) = expr else {
        return;
    };

    if !is_builtin_exception(checker, func) {
        return;
    }

    let mut diagnostic = Diagnostic::new(UselessExceptionStatement {}, expr.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        checker
            .generator()
            .stmt(&fix_useless_exception_statement(expr)),
        expr.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

fn is_builtin_exception(checker: &mut Checker, exc: &Expr) -> bool {
    return checker
        .semantic()
        .resolve_call_path(exc)
        .is_some_and(|call_path| {
            PY_BUILTIN_EXCEPTIONS.contains(call_path.as_slice().get(1).unwrap_or(&""))
        });
}

/// Generate a [`Fix`] to replace useless builtin exception `raise exception`.
///
/// For example:
/// - Given `ValueError("incorrect value")`, generate `raise ValueError("incorrect value")`.
fn fix_useless_exception_statement(expr: &Expr) -> Stmt {
    Stmt::Raise(StmtRaise {
        range: TextRange::default(),
        exc: Some(Box::new(expr.clone())),
        cause: None,
    })
}
