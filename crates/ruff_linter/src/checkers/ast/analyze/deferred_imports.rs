use ruff_python_ast::Stmt;
use rustc_hash::FxHashMap;

use crate::{checkers::ast::Checker, rules::flake8_import_conventions};

/// Analyze imports after resolving references in nested scopes and annotations.
pub(crate) fn deferred_imports(checker: &mut Checker) {
    let snapshot = checker.semantic.snapshot();
    let imports = std::mem::take(&mut checker.analyze.import_forms);
    let mut introduced = FxHashMap::default();
    for import in imports {
        checker.semantic.restore(import);
        if let Stmt::ImportFrom(stmt) = checker.semantic.current_statement() {
            flake8_import_conventions::rules::banned_import_from(checker, stmt, &mut introduced);
        }
    }
    checker.semantic.restore(snapshot);
}
