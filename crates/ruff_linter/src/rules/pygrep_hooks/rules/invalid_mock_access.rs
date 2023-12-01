use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq)]
enum Reason {
    UncalledMethod(String),
    NonExistentMethod(String),
}

/// ## What it does
/// Checks for common mistakes when using mock objects.
///
/// ## Why is this bad?
/// The `mock` module exposes an assertion API that can be used to verify that
/// mock objects undergo expected interactions. This rule checks for common
/// mistakes when using this API.
///
/// For example, it checks for mock attribute accesses that should be replaced
/// with mock method calls.
///
/// ## Example
/// ```python
/// my_mock.assert_called
/// ```
///
/// Use instead:
/// ```python
/// my_mock.assert_called()
/// ```
#[violation]
pub struct InvalidMockAccess {
    reason: Reason,
}

impl Violation for InvalidMockAccess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidMockAccess { reason } = self;
        match reason {
            Reason::UncalledMethod(name) => format!("Mock method should be called: `{name}`"),
            Reason::NonExistentMethod(name) => format!("Non-existent mock method: `{name}`"),
        }
    }
}

/// PGH005
pub(crate) fn uncalled_mock_method(checker: &mut Checker, expr: &Expr) {
    if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = expr {
        if matches!(
            attr.as_str(),
            "assert_any_call"
                | "assert_called"
                | "assert_called_once"
                | "assert_called_once_with"
                | "assert_called_with"
                | "assert_has_calls"
                | "assert_not_called"
        ) {
            checker.diagnostics.push(Diagnostic::new(
                InvalidMockAccess {
                    reason: Reason::UncalledMethod(attr.to_string()),
                },
                expr.range(),
            ));
        }
    }
}

/// PGH005
pub(crate) fn non_existent_mock_method(checker: &mut Checker, test: &Expr) {
    let attr = match test {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr,
        Expr::Call(ast::ExprCall { func, .. }) => match func.as_ref() {
            Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr,
            _ => return,
        },
        _ => return,
    };
    if matches!(
        attr.as_str(),
        "any_call"
            | "called_once"
            | "called_once_with"
            | "called_with"
            | "has_calls"
            | "not_called"
    ) {
        checker.diagnostics.push(Diagnostic::new(
            InvalidMockAccess {
                reason: Reason::NonExistentMethod(attr.to_string()),
            },
            test.range(),
        ));
    }
}
