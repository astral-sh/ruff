use rustpython_parser::ast;
use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct BadMarshalUse;

impl Violation for BadMarshalUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `marshal` module should be avoided")
    }
}

// TODO: Consider helper func for imports if more of these pop up
/// DUO120
pub(crate) fn bad_marshal_use(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport { names, range: _ }) => {
            for name in names {
                if &name.name == "marshal" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadMarshalUse, stmt.range()));
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom {module, .. }, ..) => {
            if let Some(id) = module {
                if id == "marshal" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadMarshalUse, stmt.range()));
                }
            }
        }
        _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
    }
}
