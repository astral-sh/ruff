use ruff_python_ast::Expr;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_builtins, flake8_pie, pylint};

/// Run lint rules over all deferred lambdas in the [`SemanticModel`].
pub(crate) fn deferred_lambdas(checker: &mut Checker) {
    // Note that we'll need to check for new statements in a loop if any of the rules below receive
    // a `&mut Checker` again.
    let lambdas = std::mem::take(&mut checker.analyze.lambdas);
    for snapshot in lambdas {
        checker.semantic.restore(snapshot);

        let checker = &*checker;

        let Some(Expr::Lambda(lambda)) = checker.semantic.current_expression() else {
            unreachable!("Expected Expr::Lambda");
        };

        if checker.is_rule_enabled(Rule::UnnecessaryLambda) {
            pylint::rules::unnecessary_lambda(checker, lambda);
        }
        if checker.is_rule_enabled(Rule::ReimplementedContainerBuiltin) {
            flake8_pie::rules::reimplemented_container_builtin(checker, lambda);
        }
        if checker.is_rule_enabled(Rule::BuiltinLambdaArgumentShadowing) {
            flake8_builtins::rules::builtin_lambda_argument_shadowing(checker, lambda);
        }
    }
}
