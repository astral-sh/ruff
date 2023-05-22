use rustpython_parser::ast::Identifier;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::import_from_module_range;

use crate::checkers::ast::Checker;
use crate::rules::dlint::helpers::AnyStmtImport;

/// ## What it does
/// Checks that code does not use the `marshal` module
///
/// ## Why is this bad?
/// The marshal module is not intended to be secure against erroneous or maliciously constructed
/// data. Never unmarshal data received from an untrusted or unauthenticated source.
///
/// ## Example
/// ```python
/// import marshal
/// ```
#[violation]
pub struct MarshalUse;

impl Violation for MarshalUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of the `marshal` module should be avoided")
    }
}

/// DUO120
pub(crate) fn marshal_use(checker: &mut Checker, stmt: AnyStmtImport) {
    match stmt {
        AnyStmtImport::Import(imp) => {
            for name in &imp.names {
                if name.name.as_str() == "marshal" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(MarshalUse, name.range));
                }
            }
        }
        AnyStmtImport::ImportFrom(imp) => {
            if imp.module == Some(Identifier::from("marshal")) {
                checker.diagnostics.push(Diagnostic::new(
                    MarshalUse,
                    import_from_module_range(imp, checker.locator),
                ));
            }
        }
    }
}
