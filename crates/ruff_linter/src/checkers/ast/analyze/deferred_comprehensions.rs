use ruff_python_ast::ExprKind;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::perflint;

/// Run lint rules over all deferred comprehensions in the [`SemanticModel`].
pub(crate) fn deferred_comprehensions(checker: &mut Checker) {
    let comprehensions = std::mem::take(&mut checker.analyze.comprehensions);
    for snapshot in comprehensions {
        checker.semantic.restore(snapshot);

        let checker = &*checker;

        let Some(generators) =
            checker
                .semantic
                .current_expression()
                .and_then(|expr| match expr.classify() {
                    ExprKind::ListComp(comp) => Some(comp.generators()),
                    ExprKind::SetComp(comp) => Some(comp.generators()),
                    ExprKind::DictComp(comp) => Some(comp.generators()),
                    ExprKind::Generator(generator) => Some(generator.generators()),
                    _ => None,
                })
        else {
            debug_assert!(false, "Expected a comprehension");
            continue;
        };

        for generator in generators {
            if checker.is_rule_enabled(Rule::IncorrectDictIterator) {
                perflint::rules::incorrect_dict_iterator_comprehension(checker, generator);
            }
        }
    }
}
