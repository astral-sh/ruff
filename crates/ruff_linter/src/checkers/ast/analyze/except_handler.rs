use ruff_python_ast::{self as ast, ExceptHandler};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::{
    flake8_bandit, flake8_blind_except, flake8_bugbear, flake8_builtins, pycodestyle, pylint,
};

/// Run lint rules over an [`ExceptHandler`] syntax node.
pub(crate) fn except_handler(except_handler: &ExceptHandler, checker: &Checker) {
    match except_handler {
        ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_,
            name,
            body,
            range: _,
        }) => {
            if checker.enabled(Rule::BareExcept) {
                if let Some(diagnostic) = pycodestyle::rules::bare_except(
                    type_.as_deref(),
                    body,
                    except_handler,
                    checker.locator,
                ) {
                    checker.report_diagnostic(diagnostic);
                }
            }
            if checker.enabled(Rule::RaiseWithoutFromInsideExcept) {
                flake8_bugbear::rules::raise_without_from_inside_except(
                    checker,
                    name.as_deref(),
                    body,
                );
            }
            if checker.enabled(Rule::BlindExcept) {
                flake8_blind_except::rules::blind_except(
                    checker,
                    type_.as_deref(),
                    name.as_deref(),
                    body,
                );
            }
            if checker.enabled(Rule::TryExceptPass) {
                flake8_bandit::rules::try_except_pass(
                    checker,
                    except_handler,
                    type_.as_deref(),
                    body,
                    checker.settings.flake8_bandit.check_typed_exception,
                );
            }
            if checker.enabled(Rule::TryExceptContinue) {
                flake8_bandit::rules::try_except_continue(
                    checker,
                    except_handler,
                    type_.as_deref(),
                    body,
                    checker.settings.flake8_bandit.check_typed_exception,
                );
            }
            if checker.enabled(Rule::ExceptWithEmptyTuple) {
                flake8_bugbear::rules::except_with_empty_tuple(checker, except_handler);
            }
            if checker.enabled(Rule::ExceptWithNonExceptionClasses) {
                flake8_bugbear::rules::except_with_non_exception_classes(checker, except_handler);
            }
            if checker.enabled(Rule::BinaryOpException) {
                pylint::rules::binary_op_exception(checker, except_handler);
            }
            if let Some(name) = name {
                if checker.enabled(Rule::AmbiguousVariableName) {
                    pycodestyle::rules::ambiguous_variable_name(
                        checker,
                        name.as_str(),
                        name.range(),
                    );
                }
                if checker.enabled(Rule::BuiltinVariableShadowing) {
                    flake8_builtins::rules::builtin_variable_shadowing(checker, name, name.range());
                }
            }
        }
    }
}
