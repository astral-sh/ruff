use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{helpers::ReturnStatementVisitor, statement_visitor::StatementVisitor};
use ruff_python_semantic::analyze::type_inference::PythonType;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__str__` implementations that return a type other than `str`.
///
/// ## Why is this bad?
/// The `__str__` method should return a `str` object. Returning a different
/// type may cause unexpected behavior.
#[violation]
pub struct InvalidStrReturnType;

impl Violation for InvalidStrReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__str__` does not return `str`")
    }
}

/// E0307
pub(crate) fn invalid_str_return(checker: &mut Checker, name: &str, body: &[Stmt]) {
    if name != "__str__" {
        return;
    }

    if !checker.semantic().scope().kind.is_class() {
        return;
    }

    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(body);
        visitor.returns
    };

    for stmt in returns {
        if let Some(value) = stmt.value.as_deref() {
            // Disallow other, non-
            if !matches!(
                PythonType::from(value),
                PythonType::String | PythonType::Unknown
            ) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(InvalidStrReturnType, value.range()));
            }
        } else {
            // Disallow implicit `None`.
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidStrReturnType, stmt.range()));
        }
    }
}
