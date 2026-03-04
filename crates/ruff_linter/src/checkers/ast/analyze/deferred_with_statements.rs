use ruff_python_ast::Stmt;

use crate::{checkers::ast::Checker, codes::Rule, rules::refurb};

/// Run lint rules over all deferred with-statements in the [`SemanticModel`].
pub(crate) fn deferred_with_statements(checker: &mut Checker) {
    // Note that we'll need to check for new statements in a loop if any of the rules below receive
    // a `&mut Checker` again.
    let with_statements = std::mem::take(&mut checker.analyze.with_statements);
    for snapshot in with_statements {
        checker.semantic.restore(snapshot);

        let checker = &*checker;

        let Stmt::With(stmt_with) = checker.semantic.current_statement() else {
            unreachable!("Expected Stmt::With");
        };
        if checker.is_rule_enabled(Rule::ReadWholeFile) {
            refurb::rules::read_whole_file(checker, stmt_with);
        }
        if checker.is_rule_enabled(Rule::WriteWholeFile) {
            refurb::rules::write_whole_file(checker, stmt_with);
        }
    }
}
