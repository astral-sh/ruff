use ruff_python_ast::Suite;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bugbear, ruff};

/// Run lint rules over a module.
pub(crate) fn module(suite: &Suite, checker: &Checker) {
    if checker.enabled(Rule::FStringDocstring) {
        flake8_bugbear::rules::f_string_docstring(checker, suite);
    }
    if checker.enabled(Rule::InvalidFormatterSuppressionComment) {
        ruff::rules::ignored_formatter_suppression_comment(checker, suite);
    }
}
