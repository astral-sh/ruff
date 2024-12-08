use ruff_python_ast::Stmt;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bugbear, flake8_simplify, perflint, pylint, pyupgrade, refurb};

/// Run lint rules over all deferred for-loops in the [`SemanticModel`].
pub(crate) fn deferred_for_loops(checker: &mut Checker) {
    while !checker.analyze.for_loops.is_empty() {
        let for_loops = std::mem::take(&mut checker.analyze.for_loops);
        for snapshot in for_loops {
            checker.semantic.restore(snapshot);

            let Stmt::For(stmt_for) = checker.semantic.current_statement() else {
                unreachable!("Expected Stmt::For");
            };
            if checker.enabled(Rule::UnusedLoopControlVariable) {
                flake8_bugbear::rules::unused_loop_control_variable(checker, stmt_for);
            }
            if checker.enabled(Rule::IncorrectDictIterator) {
                perflint::rules::incorrect_dict_iterator(checker, stmt_for);
            }
            if checker.enabled(Rule::YieldInForLoop) {
                pyupgrade::rules::yield_in_for_loop(checker, stmt_for);
            }
            if checker.enabled(Rule::UnnecessaryEnumerate) {
                refurb::rules::unnecessary_enumerate(checker, stmt_for);
            }
            if checker.enabled(Rule::EnumerateForLoop) {
                flake8_simplify::rules::enumerate_for_loop(checker, stmt_for);
            }
            if checker.enabled(Rule::LoopIteratorMutation) {
                flake8_bugbear::rules::loop_iterator_mutation(checker, stmt_for);
            }
            if checker.enabled(Rule::DictIndexMissingItems) {
                pylint::rules::dict_index_missing_items(checker, stmt_for);
            }
            if checker.enabled(Rule::ManualListComprehension) {
                perflint::rules::manual_list_comprehension(checker, stmt_for);
            }
        }
    }
}
