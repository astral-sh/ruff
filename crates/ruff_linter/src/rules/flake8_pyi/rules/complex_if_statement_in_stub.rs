use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `if` statements with complex conditionals in stubs.
///
/// ## Why is this bad?
/// Stub files support simple conditionals to test for differences in Python
/// versions and platforms. However, type checkers only understand a limited
/// subset of these conditionals; complex conditionals may result in false
/// positives or false negatives.
///
/// ## Example
/// ```python
/// import sys
///
/// if (2, 7) < sys.version_info < (3, 5):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info < (3, 5):
///     ...
/// ```
#[violation]
pub struct ComplexIfStatementInStub;

impl Violation for ComplexIfStatementInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`if` test must be a simple comparison against `sys.platform` or `sys.version_info`"
        )
    }
}

/// PYI002
pub(crate) fn complex_if_statement_in_stub(checker: &mut Checker, test: &Expr) {
    let Expr::Compare(ast::ExprCompare {
        left, comparators, ..
    }) = test
    else {
        checker
            .diagnostics
            .push(Diagnostic::new(ComplexIfStatementInStub, test.range()));
        return;
    };

    if comparators.len() != 1 {
        checker
            .diagnostics
            .push(Diagnostic::new(ComplexIfStatementInStub, test.range()));
        return;
    }

    if left.is_subscript_expr() {
        return;
    }

    if checker
        .semantic()
        .resolve_call_path(left)
        .is_some_and(|call_path| {
            matches!(call_path.as_slice(), ["sys", "version_info" | "platform"])
        })
    {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(ComplexIfStatementInStub, test.range()));
}
