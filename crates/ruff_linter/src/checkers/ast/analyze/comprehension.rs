use ruff_python_ast::Comprehension;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_simplify, refurb};

/// Run lint rules over a [`Comprehension`] syntax nodes.
pub(crate) fn comprehension<'a>(comprehension: &'a Comprehension, checker: &mut Checker<'a>) {
    if checker.is_rule_enabled(Rule::InDictKeys) {
        flake8_simplify::rules::key_in_dict_comprehension(checker, comprehension);
    }
    if checker.is_rule_enabled(Rule::ReadlinesInFor) {
        refurb::rules::readlines_in_comprehension(checker, comprehension);
    }
    if checker.is_rule_enabled(Rule::IncorrectDictIterator) {
        checker
            .analyze
            .comprehensions
            .push((comprehension, checker.semantic.snapshot()));
    }
}
