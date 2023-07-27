use ruff_python_ast::{Arg, Ranged};

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_builtins, pep8_naming, pycodestyle};

/// Run lint rules over an [`Arg`] syntax node.
pub(crate) fn argument(arg: &Arg, checker: &mut Checker) {
    if checker.enabled(Rule::AmbiguousVariableName) {
        if let Some(diagnostic) = pycodestyle::rules::ambiguous_variable_name(&arg.arg, arg.range())
        {
            checker.diagnostics.push(diagnostic);
        }
    }
    if checker.enabled(Rule::InvalidArgumentName) {
        if let Some(diagnostic) = pep8_naming::rules::invalid_argument_name(
            &arg.arg,
            arg,
            &checker.settings.pep8_naming.ignore_names,
        ) {
            checker.diagnostics.push(diagnostic);
        }
    }
    if checker.enabled(Rule::BuiltinArgumentShadowing) {
        flake8_builtins::rules::builtin_argument_shadowing(checker, arg);
    }
}
