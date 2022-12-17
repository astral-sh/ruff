use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLR0402
pub fn use_from_import(checker: &mut Checker, alias: &Alias) {
    let Some(asname) = &alias.node.asname else {
        return;
    };
    let Some((module, name)) = alias.node.name.rsplit_once('.') else {
        return;
    };
    if name != asname {
        return;
    }
    checker.add_check(Check::new(
        CheckKind::ConsiderUsingFromImport(module.to_string(), name.to_string()),
        Range::from_located(alias),
    ));
}
