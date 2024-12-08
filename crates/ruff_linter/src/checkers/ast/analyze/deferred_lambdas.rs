use ruff_python_ast::Expr;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_builtins, flake8_pie, pylint};

/// Run lint rules over all deferred lambdas in the [`SemanticModel`].
pub(crate) fn deferred_lambdas(checker: &mut Checker) {
    while !checker.analyze.lambdas.is_empty() {
        let lambdas = std::mem::take(&mut checker.analyze.lambdas);
        for snapshot in lambdas {
            checker.semantic.restore(snapshot);

            let Some(Expr::Lambda(lambda)) = checker.semantic.current_expression() else {
                unreachable!("Expected Expr::Lambda");
            };

            if checker.enabled(Rule::UnnecessaryLambda) {
                pylint::rules::unnecessary_lambda(checker, lambda);
            }
            if checker.enabled(Rule::ReimplementedContainerBuiltin) {
                flake8_pie::rules::reimplemented_container_builtin(checker, lambda);
            }
            if checker.enabled(Rule::BuiltinLambdaArgumentShadowing) {
                flake8_builtins::rules::builtin_lambda_argument_shadowing(checker, lambda);
            }
        }
    }
}
