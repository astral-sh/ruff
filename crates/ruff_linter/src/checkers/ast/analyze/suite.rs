use ruff_python_ast::Stmt;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::refurb;
use crate::rules::{flake8_pie, flake8_pyi};

/// Run lint rules over a suite of [`Stmt`] syntax nodes.
pub(crate) fn suite(suite: &[Stmt], checker: &Checker) {
    if checker.is_rule_enabled(Rule::UnnecessaryPlaceholder) {
        flake8_pie::rules::unnecessary_placeholder(checker, suite);
    }
    if checker.source_type.is_stub() && checker.is_rule_enabled(Rule::DocstringInStub) {
        flake8_pyi::rules::docstring_in_stubs(checker, suite);
    }
    if checker.is_rule_enabled(Rule::RepeatedGlobal) {
        refurb::rules::repeated_global(checker, suite);
    }
}
