use ruff_python_ast::Suite;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bugbear, ruff, ssort};

/// Run lint rules over a module.
pub(crate) fn module(suite: &Suite, checker: &Checker) {
    if checker.is_rule_enabled(Rule::FStringDocstring) {
        flake8_bugbear::rules::f_string_docstring(checker, suite);
    }
    if checker.is_rule_enabled(Rule::UnsortedStatements) {
        ssort::rules::organize_statements(checker, suite);
    }
    if checker.is_rule_enabled(Rule::FunctionCallCycle) {
        ssort::rules::detect_function_cycle(checker, suite);
    }
    if checker.is_rule_enabled(Rule::InvalidFormatterSuppressionComment) {
        ruff::rules::ignored_formatter_suppression_comment(checker, suite);
    }
}
