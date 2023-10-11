use ruff_python_ast::Alias;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for import aliases that do not rename the original package.
///
/// ## Why is this bad?
/// The import alias is redundant and should be removed to avoid confusion.
///
/// ## Example
/// ```python
/// import numpy as numpy
/// ```
///
/// Use instead:
/// ```python
/// import numpy as np
/// ```
#[violation]
pub struct UselessImportAlias;

impl AlwaysFixableViolation for UselessImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Import alias does not rename original package")
    }

    fn fix_title(&self) -> String {
        "Remove import alias".to_string()
    }
}

/// PLC0414
pub(crate) fn useless_import_alias(checker: &mut Checker, alias: &Alias) {
    let Some(asname) = &alias.asname else {
        return;
    };
    if alias.name.contains('.') {
        return;
    }
    if alias.name.as_str() != asname.as_str() {
        return;
    }

    let mut diagnostic = Diagnostic::new(UselessImportAlias, alias.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        asname.to_string(),
        alias.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
