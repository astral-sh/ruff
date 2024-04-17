use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::Stmt;
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__bytes__` implementations that return a type other than `bytes`.
///
/// ## Why is this bad?
/// The `__bytes__` method should return a `bytes` object. Returning a different
/// type may cause unexpected behavior.
///
/// ## Example
/// ```python
/// class Foo:
///     def __bytes__(self):
///         return 2
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __bytes__(self):
///         return b"2"
/// ```
///
/// ## References
/// - [Python documentation: The `__bytes__` method](https://docs.python.org/3/reference/datamodel.html#object.__bytes__)
#[violation]
pub struct InvalidBytesReturnType;

impl Violation for InvalidBytesReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__bytes__` does not return `bytes`")
    }
}

/// E0308
pub(crate) fn invalid_bytes_return(checker: &mut Checker, name: &str, body: &[Stmt]) {
    if name != "__bytes__" {
        return;
    }

    if !checker.semantic().current_scope().kind.is_class() {
        return;
    }

    if body.len() == 1
        && (matches!(&body[0], Stmt::Expr(expr) if expr.value.is_ellipsis_literal_expr())
            || body[0].is_pass_stmt()
            || body[0].is_raise_stmt())
    {
        return;
    }

    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(body);
        visitor.returns
    };

    if returns.is_empty() {
        checker.diagnostics.push(Diagnostic::new(
            InvalidBytesReturnType,
            body.last().unwrap().range(),
        ));
    }

    for stmt in returns {
        if let Some(value) = stmt.value.as_deref() {
            if !matches!(
                ResolvedPythonType::from(value),
                ResolvedPythonType::Unknown | ResolvedPythonType::Atom(PythonType::Bytes)
            ) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(InvalidBytesReturnType, value.range()));
            }
        } else {
            // Disallow implicit `None`.
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidBytesReturnType, stmt.range()));
        }
    }
}
