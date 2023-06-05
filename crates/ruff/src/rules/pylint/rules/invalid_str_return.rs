use rustpython_parser::ast::{self, Constant, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{helpers::ReturnStatementVisitor, statement_visitor::StatementVisitor};

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

    if !checker.semantic_model().scope().kind.is_class() {
        return;
    }

    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(body);
        visitor.returns
    };

    for stmt in returns {
        // Disallow implicit `None`.
        let Some(value) = stmt.value.as_deref() else {
            checker.diagnostics.push(Diagnostic::new(InvalidStrReturnType, stmt.range()));
            continue;
        };

        // Disallow other constants.
        if matches!(
            value,
            Expr::List(_)
                | Expr::Dict(_)
                | Expr::Set(_)
                | Expr::ListComp(_)
                | Expr::DictComp(_)
                | Expr::SetComp(_)
                | Expr::GeneratorExp(_)
                | Expr::Constant(ast::ExprConstant {
                    value: Constant::None
                        | Constant::Bool(_)
                        | Constant::Bytes(_)
                        | Constant::Int(_)
                        | Constant::Tuple(_)
                        | Constant::Float(_)
                        | Constant::Complex { .. }
                        | Constant::Ellipsis,
                    ..
                })
        ) {
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidStrReturnType, value.range()));
        }
    }
}
