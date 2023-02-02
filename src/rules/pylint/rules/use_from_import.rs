use rustpython_ast::Alias;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;

define_violation!(
    pub struct ConsiderUsingFromImport {
        pub module: String,
        pub name: String,
    }
);
impl Violation for ConsiderUsingFromImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderUsingFromImport { module, name } = self;
        format!("Use `from {module} import {name}` in lieu of alias")
    }
}
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
    checker.diagnostics.push(Diagnostic::new(
        ConsiderUsingFromImport {
            module: module.to_string(),
            name: name.to_string(),
        },
        Range::from_located(alias),
    ));
}
