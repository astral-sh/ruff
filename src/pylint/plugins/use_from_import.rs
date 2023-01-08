use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// PLR0402
pub fn use_from_import(xxxxxxxx: &mut xxxxxxxx, alias: &Alias) {
    let Some(asname) = &alias.node.asname else {
        return;
    };
    let Some((module, name)) = alias.node.name.rsplit_once('.') else {
        return;
    };
    if name != asname {
        return;
    }
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::ConsiderUsingFromImport(module.to_string(), name.to_string()),
        Range::from_located(alias),
    ));
}
