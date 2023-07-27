use ruff_python_ast::Arguments;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bugbear, flake8_pyi, ruff};

/// Run lint rules over a [`Arguments`] syntax node.
pub(crate) fn arguments(arguments: &Arguments, checker: &mut Checker) {
    if checker.enabled(Rule::MutableArgumentDefault) {
        flake8_bugbear::rules::mutable_argument_default(checker, arguments);
    }
    if checker.enabled(Rule::FunctionCallInDefaultArgument) {
        flake8_bugbear::rules::function_call_in_argument_default(checker, arguments);
    }
    if checker.settings.rules.enabled(Rule::ImplicitOptional) {
        ruff::rules::implicit_optional(checker, arguments);
    }
    if checker.is_stub {
        if checker.enabled(Rule::TypedArgumentDefaultInStub) {
            flake8_pyi::rules::typed_argument_simple_defaults(checker, arguments);
        }
        if checker.enabled(Rule::ArgumentDefaultInStub) {
            flake8_pyi::rules::argument_simple_defaults(checker, arguments);
        }
    }
}
