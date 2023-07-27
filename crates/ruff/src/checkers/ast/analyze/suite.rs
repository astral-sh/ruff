use ruff_python_ast::Stmt;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::flake8_pie;

/// Run lint rules over a suite of [`Stmt`] syntax nodes.
pub(crate) fn suite(suite: &[Stmt], checker: &mut Checker) {
    if checker.enabled(Rule::UnnecessaryPass) {
        flake8_pie::rules::no_unnecessary_pass(checker, suite);
    }
}
