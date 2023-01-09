use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::{violations, Diagnostic};

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

    let mut diagnostic =
        Diagnostic::new(violations::UselessImportAlias, Range::from_located(alias));
    if checker.patch(diagnostic.kind.code()) {
        diagnostic.amend(Fix::replacement(
            asname.to_string(),
            alias.location,
            alias.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
