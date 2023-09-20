use ruff_python_ast::Suite;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::flake8_bugbear;

/// Run lint rules over a module.
pub(crate) fn module(suite: &Suite, checker: &mut Checker) {
    if checker.enabled(Rule::FStringDocstring) {
        flake8_bugbear::rules::f_string_docstring(checker, suite);
    }
}
