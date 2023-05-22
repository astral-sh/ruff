use rustpython_parser::ast::Identifier;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::dlint::helpers::AnyStmtImport;

/// ## What it does
/// Checks that code does not use the `shelve` module
///
/// ## Why is this bad?
/// Because the shelve module is backed by pickle, it is insecure to load a shelf from an
/// untrusted source. Like with pickle, loading a shelf can execute arbitrary code.
///
/// ## Example
/// ```python
/// import shelve
/// ```
#[violation]
pub struct ShelveUse;

impl Violation for ShelveUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `shelve` module should be avoided")
    }
}

/// DUO119
pub(crate) fn bad_shelve_use(checker: &mut Checker, stmt: AnyStmtImport) {
    match stmt {
        AnyStmtImport::Import(imp) => {
            for name in &imp.names {
                if name.name.as_str() == "shelve" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(ShelveUse, name.range));
                }
            }
        }
        AnyStmtImport::ImportFrom(imp) => {
            if imp.module == Some(Identifier::from("shelve")) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(ShelveUse, imp.range));
            }
        }
    }
}
