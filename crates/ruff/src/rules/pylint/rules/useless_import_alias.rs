use rustpython_parser::ast::Alias;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UselessImportAlias;

impl AlwaysAutofixableViolation for UselessImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Import alias does not rename original package")
    }

    fn autofix_title(&self) -> String {
        "Remove import alias".to_string()
    }
}

/// PLC0414
pub fn useless_import_alias(checker: &mut Checker, alias: &Alias) {
    let Some(asname) = &alias.node.asname else {
        return;
    };
    if alias.node.name.contains('.') {
        return;
    }
    if &alias.node.name != asname {
        return;
    }

    let mut diagnostic = Diagnostic::new(UselessImportAlias, alias.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::range_replacement(asname.to_string(), alias.range()));
    }
    checker.diagnostics.push(diagnostic);
}
