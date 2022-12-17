use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

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

    let mut check = Check::new(CheckKind::UselessImportAlias, Range::from_located(alias));
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            asname.to_string(),
            alias.location,
            alias.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}
