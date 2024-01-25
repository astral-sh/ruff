use ruff_python_ast::Suite;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::flake8_bugbear;
use crate::rules::pylint;

/// Run lint rules over a module.
pub(crate) fn module(suite: &Suite, checker: &mut Checker) {
    if checker.enabled(Rule::FStringDocstring) {
        flake8_bugbear::rules::f_string_docstring(checker, suite);
    }
    if checker.enabled(Rule::RedefinedSlotsInSubclass) {
        pylint::rules::redefined_slots_in_subclass(checker, suite);
    }
}
