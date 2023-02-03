use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Alias, Stmt};

use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_simple_violation!(MultipleImportsOnOneLine, "Multiple imports on one line");

define_simple_violation!(
    ModuleImportNotAtTopOfFile,
    "Module level import not at top of file"
);

pub fn multiple_imports_on_one_line(checker: &mut Checker, stmt: &Stmt, names: &[Alias]) {
    if names.len() > 1 {
        checker.diagnostics.push(Diagnostic::new(
            MultipleImportsOnOneLine,
            Range::from_located(stmt),
        ));
    }
}

pub fn module_import_not_at_top_of_file(checker: &mut Checker, stmt: &Stmt) {
    if checker.seen_import_boundary && stmt.location.column() == 0 {
        checker.diagnostics.push(Diagnostic::new(
            ModuleImportNotAtTopOfFile,
            Range::from_located(stmt),
        ));
    }
}
