use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Alias, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::{Availability, Violation};
use crate::AutofixKind;

define_violation!(
    pub struct ConsiderUsingFromImport {
        pub module: String,
        pub name: String,
        pub fixable: bool,
    }
);
impl Violation for ConsiderUsingFromImport {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderUsingFromImport { module, name, .. } = self;
        format!("Use `from {module} import {name}` in lieu of alias")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let ConsiderUsingFromImport { fixable, .. } = self;
        if *fixable {
            Some(|ConsiderUsingFromImport { module, name, .. }| {
                format!("Replace with `from {module} import {name}`")
            })
        } else {
            None
        }
    }
}

/// PLR0402
pub fn use_from_import(checker: &mut Checker, stmt: &Stmt, alias: &Alias, names: &[Alias]) {
    let Some(asname) = &alias.node.asname else {
        return;
    };
    let Some((module, name)) = alias.node.name.rsplit_once('.') else {
        return;
    };
    if name != asname {
        return;
    }

    let fixable = names.len() == 1;
    let mut diagnostic = Diagnostic::new(
        ConsiderUsingFromImport {
            module: module.to_string(),
            name: name.to_string(),
            fixable,
        },
        Range::from_located(alias),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            format!("from {module} import {asname}"),
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
