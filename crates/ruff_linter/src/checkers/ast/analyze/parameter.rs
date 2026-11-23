use ruff_python_ast::Parameter;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_builtins, pycodestyle};

/// Run lint rules over a [`Parameter`] syntax node.
pub(crate) fn parameter(parameter: &Parameter, checker: &Checker) {
    if checker.is_rule_enabled(Rule::AmbiguousVariableName) {
        pycodestyle::rules::ambiguous_variable_name(
            checker,
            &parameter.name,
            parameter.name.range(),
        );
    }
    if checker.is_rule_enabled(Rule::BuiltinParameterShadowing) {
        flake8_builtins::rules::builtin_parameter_shadowing(checker, parameter);
    }
}
