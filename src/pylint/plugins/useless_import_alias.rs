use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// PLC0414
pub fn useless_import_alias(xxxxxxxx: &mut xxxxxxxx, alias: &Alias) {
    let Some(asname) = &alias.node.asname else {
        return;
    };
    if alias.node.name.contains('.') {
        return;
    }
    if &alias.node.name != asname {
        return;
    }

    let mut check = Diagnostic::new(violations::UselessImportAlias, Range::from_located(alias));
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            asname.to_string(),
            alias.location,
            alias.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}
