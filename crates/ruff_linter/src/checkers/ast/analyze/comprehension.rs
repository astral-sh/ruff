use ruff_python_ast::Comprehension;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::flake8_simplify;

/// Run lint rules over a [`Comprehension`] syntax nodes.
pub(crate) fn comprehension(comprehension: &Comprehension, checker: &mut Checker) {
    if checker.enabled(Rule::InDictKeys) {
        flake8_simplify::rules::key_in_dict_comprehension(checker, comprehension);
    }
}
