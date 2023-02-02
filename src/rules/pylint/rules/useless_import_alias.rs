use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;

define_violation!(
    pub struct UselessImportAlias;
);
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

    let mut diagnostic = Diagnostic::new(UselessImportAlias, Range::from_located(alias));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            asname.to_string(),
            alias.location,
            alias.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
