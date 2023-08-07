use ruff_python_ast::{self as ast, Stmt};

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_bugbear, perflint};

/// Run lint rules over all deferred for-loops in the [`SemanticModel`].
pub(crate) fn deferred_for_loops(checker: &mut Checker) {
    while !checker.deferred.for_loops.is_empty() {
        let for_loops = std::mem::take(&mut checker.deferred.for_loops);
        for snapshot in for_loops {
            checker.semantic.restore(snapshot);

            let Stmt::For(ast::StmtFor {
                target, iter, body, ..
            }) = checker.semantic.current_statement()
            else {
                unreachable!("Expected Stmt::For");
            };

            if checker.enabled(Rule::UnusedLoopControlVariable) {
                flake8_bugbear::rules::unused_loop_control_variable(checker, target, body);
            }
            if checker.enabled(Rule::IncorrectDictIterator) {
                perflint::rules::incorrect_dict_iterator(checker, target, iter);
            }
        }
    }
}
