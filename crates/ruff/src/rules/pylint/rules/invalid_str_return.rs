use rustpython_parser::ast::{Expr, Ranged, Stmt};

use ruff_python_ast::{helpers::ReturnStatementVisitor, statement_visitor::StatementVisitor};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Returns Err when __str__ method returns something which is not a string
///
/// ## Why is this bad?
/// __str__ method should only return str type
///
///
#[violation]
pub struct InvalidStrReturnType;

impl Violation for InvalidStrReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("__str__ does not return str")
    }
}

// checks for str return type
fn is_str_returning(body: &[Stmt]) -> Option<Diagnostic> {
    let mut visitor = ReturnStatementVisitor::default();
    visitor.visit_body(body);
    for expr in visitor.returns.into_iter().flatten() {
        if matches!(
            expr,
            Expr::Constant(ref constant) if constant.value.is_bool() || constant.value.is_int() || constant.value.is_tuple() || constant.value.is_float()
        ) {
            return Some(Diagnostic::new(InvalidStrReturnType, expr.range()));
        }
    }
    None
}

/// E0307
pub(crate) fn invalid_str_return(checker: &mut Checker, name: &str, body: &[Stmt]) {
    if name != "__str__" {
        return;
    }
    if let Some(non_str_return_type) = is_str_returning(body) {
        checker.diagnostics.push(non_str_return_type);
    }
}
