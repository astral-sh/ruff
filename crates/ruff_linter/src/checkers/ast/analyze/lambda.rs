use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::pylint;
use ruff_python_ast::{self as ast, Expr};

pub(crate) fn lambda(expr: &Expr, checker: &mut Checker) {
    match expr {
        Expr::Lambda(
            lambda @ ast::ExprLambda {
                parameters: _,
                body: _,
                range: _,
            },
        ) => {
            if checker.enabled(Rule::UnnecessaryLambda) {
                pylint::rules::unnecessary_lambda(checker, lambda);
            }
        }
        _ => unreachable!("Expected Expr::Lambda"),
    }
}
