use crate::checkers::ast::Checker;
use crate::rules::flake8_bugbear;

/// Analyze preview B014 checks after all function-local bindings are known.
pub(crate) fn deferred_duplicate_exceptions(checker: &mut Checker) {
    let snapshots = std::mem::take(&mut checker.analyze.duplicate_exceptions);
    for snapshot in snapshots {
        checker.semantic.restore(snapshot);
        let Some(try_stmt) = checker.semantic().current_statement().as_try_stmt() else {
            unreachable!("Expected Stmt::Try");
        };
        flake8_bugbear::rules::duplicate_exceptions(checker, &try_stmt.handlers);
    }
}
