use rustpython_ast::Alias;
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;

define_violation!(
    pub struct ConsiderUsingFromImport {
        pub module: String,
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for ConsiderUsingFromImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderUsingFromImport { module, name } = self;
        format!("Use `from {module} import {name}` in lieu of alias")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `from {module} import {name}`".to_string()
    }
}
/// PLR0402
pub fn use_from_import(checker: &mut Checker, alias: &Alias, import_stmt: &Stmt, names: &[Alias]) {
    let Some(asname) = &alias.node.asname else {
        return;
    };
    let Some((module, name)) = alias.node.name.rsplit_once('.') else {
        return;
    };
    if name != asname {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        ConsiderUsingFromImport {
            module: module.to_string(),
            name: name.to_string(),
        },
        Range::from_located(alias),
    );
    if checker.patch(diagnostic.kind.rule()) && names.len() == 1 {
        diagnostic.amend(Fix::replacement(
            format!("from {module} import {asname}"),
            import_stmt.location,
            import_stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
