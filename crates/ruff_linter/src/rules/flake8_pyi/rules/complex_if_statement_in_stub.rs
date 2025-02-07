use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
/// - [Typing documentation: Version and platform checking](https://typing.readthedocs.io/en/latest/spec/directives.html#version-and-platform-checks)
#[derive(ViolationMetadata)]
pub(crate) struct ComplexIfStatementInStub;

impl Violation for ComplexIfStatementInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`if` test must be a simple comparison against `sys.platform` or `sys.version_info`"
            .to_string()
    }
}

/// PYI002
pub(crate) fn complex_if_statement_in_stub(checker: &Checker, test: &Expr) {
    let Expr::Compare(ast::ExprCompare {
        left, comparators, ..
    }) = test
    else {
        checker.report_diagnostic(Diagnostic::new(ComplexIfStatementInStub, test.range()));
        return;
    };

    if comparators.len() != 1 {
        checker.report_diagnostic(Diagnostic::new(ComplexIfStatementInStub, test.range()));
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

    checker.report_diagnostic(Diagnostic::new(ComplexIfStatementInStub, test.range()));
}
