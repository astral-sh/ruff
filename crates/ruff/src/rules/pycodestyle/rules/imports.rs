use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Alias, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct MultipleImportsOnOneLine;
);
impl Violation for MultipleImportsOnOneLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple imports on one line")
    }
}

define_violation!(
    pub struct ModuleImportNotAtTopOfFile;
);
impl Violation for ModuleImportNotAtTopOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Module level import not at top of file")
    }
}

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
