use rustpython_parser::ast::StmtImportFrom;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct ImportFromFuture;

impl Violation for ImportFromFuture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`from __future__` import annotations has no effect in stub files, since type checkers automatically treat stubs as having those semantics")
    }
}

/// PYI044
pub(crate) fn from_future_import(checker: &mut Checker, target: &StmtImportFrom) {
    if let StmtImportFrom {
        range,
        module: Some(name),
        ..
    } = target
    {
        if name == "__future__" {
            checker
                .diagnostics
                .push(Diagnostic::new(ImportFromFuture, *range));
        }
    }
}
