use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::Stmt;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__bool__` implementations that return a type other than `bool`.
///
/// ## Why is this bad?
/// The `__bool__` method should return a `bool` object. Returning a different
/// type may cause unexpected behavior.
///
/// ## Example
/// ```python
/// class Foo:
///     def __bool__(self):
///         return 2
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __bool__(self):
///         return True
/// ```
///
/// ## References
/// - [Python documentation: The `__bool__` method](https://docs.python.org/3/reference/datamodel.html#object.__bool__)
#[violation]
pub struct InvalidBoolReturnType;

impl Violation for InvalidBoolReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__bool__` does not return `bool`")
    }
}

/// E0307
pub(crate) fn invalid_bool_return(checker: &mut Checker, name: &str, body: &[Stmt]) {
    if name != "__bool__" {
        return;
    }

    if !checker.semantic().current_scope().kind.is_class() {
        return;
    }

    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(body);
        visitor.returns
    };

    for stmt in returns {
        if let Some(value) = stmt.value.as_deref() {
            if !matches!(
                ResolvedPythonType::from(value),
                ResolvedPythonType::Unknown
                    | ResolvedPythonType::Atom(PythonType::Number(NumberLike::Bool))
            ) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(InvalidBoolReturnType, value.range()));
            }
        } else {
            // Disallow implicit `None`.
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidBoolReturnType, stmt.range()));
        }
    }
}
