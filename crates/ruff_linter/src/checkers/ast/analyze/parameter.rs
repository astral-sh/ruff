use ruff_python_ast::Parameter;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_builtins, pycodestyle};

/// Run lint rules over a [`Parameter`] syntax node.
pub(crate) fn parameter(parameter: &Parameter, checker: &Checker) {
    if checker.enabled(Rule::AmbiguousVariableName) {
        pycodestyle::rules::ambiguous_variable_name(
            checker,
            &parameter.name,
            parameter.name.range(),
        );
    }
    if checker.enabled(Rule::BuiltinArgumentShadowing) {
        flake8_builtins::rules::builtin_argument_shadowing(checker, parameter);
    }
}
