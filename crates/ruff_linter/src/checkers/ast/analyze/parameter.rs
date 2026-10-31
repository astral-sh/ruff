use ruff_python_ast::Parameter;
use ruff_python_ast::flat;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_builtins, pycodestyle};

/// Run lint rules over a [`Parameter`] syntax node.
pub(crate) fn parameter(parameter: flat::View<'_, Parameter>, checker: &Checker) {
    if checker.is_rule_enabled(Rule::AmbiguousVariableName) {
        pycodestyle::rules::ambiguous_variable_name(
            checker,
            parameter.name().as_str(),
            parameter.name().range(),
        );
    }
    if checker.is_rule_enabled(Rule::BuiltinArgumentShadowing) {
        flake8_builtins::rules::builtin_argument_shadowing(checker, parameter);
    }
}
