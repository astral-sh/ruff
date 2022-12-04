use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLR0402
pub fn consider_using_from_import(checker: &mut Checker, alias: &Alias) {
    if let Some(asname) = &alias.node.asname {
        if let Some((module, name)) = alias.node.name.rsplit_once('.') {
            if name == asname {
                checker.add_check(Check::new(
                    CheckKind::ConsiderUsingFromImport(module.to_string(), name.to_string()),
                    Range::from_located(alias),
                ));
            }
        }
    }
}
