use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `if` statements with complex conditionals in stubs.
///
/// ## Why is this bad?
/// Type checkers understand simple conditionals to express variations between
/// different Python versions and platforms. However, complex tests may not be
/// understood by a type checker, leading to incorrect inferences when they
/// analyze your code.
///
/// ## Example
/// ```pyi
/// import sys
///
/// if (3, 10) <= sys.version_info < (3, 12): ...
/// ```
///
/// Use instead:
/// ```pyi
/// import sys
///
/// if sys.version_info >= (3, 10) and sys.version_info < (3, 12): ...
/// ```
///
/// ## References
/// The [typing documentation on stub files](https://typing.readthedocs.io/en/latest/source/stubs.html#version-and-platform-checks)
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
        .resolve_qualified_name(left)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["sys", "version_info" | "platform"]
            )
        })
    {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(ComplexIfStatementInStub, test.range()));
}
