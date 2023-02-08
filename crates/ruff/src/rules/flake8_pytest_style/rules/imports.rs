use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct IncorrectPytestImport;
);
impl Violation for IncorrectPytestImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found incorrect import of pytest, use simple `import pytest` instead")
    }
}

fn is_pytest_or_subpackage(imported_name: &str) -> bool {
    imported_name == "pytest" || imported_name.starts_with("pytest.")
}

/// PT013
pub fn import(import_from: &Stmt, name: &str, asname: Option<&str>) -> Option<Diagnostic> {
    if is_pytest_or_subpackage(name) {
        if let Some(alias) = asname {
            if alias != name {
                return Some(Diagnostic::new(
                    IncorrectPytestImport,
                    Range::from_located(import_from),
                ));
            }
        }
    }
    None
}

/// PT013
pub fn import_from(
    import_from: &Stmt,
    module: Option<&str>,
    level: Option<&usize>,
) -> Option<Diagnostic> {
    // If level is not zero or module is none, return
    if let Some(level) = level {
        if *level != 0 {
            return None;
        }
    };

    if let Some(module) = module {
        if is_pytest_or_subpackage(module) {
            return Some(Diagnostic::new(
                IncorrectPytestImport,
                Range::from_located(import_from),
            ));
        }
    };

    None
}
