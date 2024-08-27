use ruff_python_ast::Parameter;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_builtins, pep8_naming, pycodestyle};

/// Run lint rules over a [`Parameter`] syntax node.
pub(crate) fn parameter(parameter: &Parameter, checker: &mut Checker) {
    if checker.enabled(Rule::AmbiguousVariableName) {
        pycodestyle::rules::ambiguous_variable_name(
            checker,
            &parameter.name,
            parameter.name.range(),
        );
    }
    if checker.enabled(Rule::InvalidArgumentName) {
        if let Some(diagnostic) = pep8_naming::rules::invalid_argument_name(
            &parameter.name,
            parameter,
            &checker.settings.pep8_naming.ignore_names,
        ) {
            checker.diagnostics.push(diagnostic);
        }
    }
    if checker.enabled(Rule::BuiltinArgumentShadowing) {
        flake8_builtins::rules::builtin_argument_shadowing(checker, parameter);
    }
}
