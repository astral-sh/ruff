use rustpython_parser::ast;
use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct BadShelveUse;

impl Violation for BadShelveUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `shelve` module should be avoided")
    }
}

/// DUO119
pub(crate) fn bad_shelve_use(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport { names, range: _ }) => {
            for name in names {
                if &name.name == "shelve" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadShelveUse, stmt.range()));
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom {module, .. }, ..) => {
            if let Some(id) = module {
                if id == "shelve" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadShelveUse, stmt.range()));
                }
            }
        }
        _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
    }
}
