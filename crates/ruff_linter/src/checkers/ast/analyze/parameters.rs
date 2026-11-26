use ruff_python_ast::Parameters;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bugbear, flake8_pyi, ruff};

/// Run lint rules over a [`Parameters`] syntax node.
pub(crate) fn parameters(parameters: &Parameters, checker: &Checker) {
    if checker.is_rule_enabled(Rule::FunctionCallInParameterDefault) {
        flake8_bugbear::rules::function_call_in_parameter_default(checker, parameters);
    }
    if checker.is_rule_enabled(Rule::ImplicitOptional) {
        ruff::rules::implicit_optional(checker, parameters);
    }
    if checker.source_type.is_stub() {
        if checker.is_rule_enabled(Rule::ComplexDefaultForAnnotatedParameterInStub) {
            flake8_pyi::rules::typed_argument_simple_defaults(checker, parameters);
        }
        if checker.is_rule_enabled(Rule::ComplexDefaultForUnannotatedParameterInStub) {
            flake8_pyi::rules::argument_simple_defaults(checker, parameters);
        }
    }
}
