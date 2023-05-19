use rustpython_parser::ast::Identifier;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::dlint::helpers::AnyStmtImport;

#[violation]
pub struct BadMarshalUse;

impl Violation for BadMarshalUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `marshal` module should be avoided")
    }
}

/// DUO120
pub(crate) fn bad_marshal_use(checker: &mut Checker, stmt: AnyStmtImport) {
    match stmt {
        AnyStmtImport::Import(imp) => {
            for name in &imp.names {
                if name.name.as_str() == "marshal" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadMarshalUse, name.range));
                }
            }
        }
        AnyStmtImport::ImportFrom(imp) => {
            if imp.module == Some(Identifier::from("marshal")) {
                for name in &imp.names {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BadMarshalUse, name.range));
                }
            }
        }
    }
}
