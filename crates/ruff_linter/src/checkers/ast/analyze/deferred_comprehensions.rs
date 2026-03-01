use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::perflint;

/// Run lint rules over all deferred comprehensions in the [`SemanticModel`].
pub(crate) fn deferred_comprehensions(checker: &mut Checker) {
    while !checker.analyze.comprehensions.is_empty() {
        let comprehensions = std::mem::take(&mut checker.analyze.comprehensions);
        for (comprehension, snapshot) in comprehensions {
            checker.semantic.restore(snapshot);

            if checker.is_rule_enabled(Rule::IncorrectDictIterator) {
                perflint::rules::incorrect_dict_iterator_comprehension(checker, comprehension);
            }
        }
    }
}
