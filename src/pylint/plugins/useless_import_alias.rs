use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLC0414
pub fn useless_import_alias(checker: &mut Checker, alias: &Alias) {
    if let Some(asname) = &alias.node.asname {
        if !alias.node.name.contains('.') && &alias.node.name == asname {
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
    }
}
