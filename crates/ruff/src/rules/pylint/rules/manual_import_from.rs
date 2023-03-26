use rustpython_parser::ast::{Alias, AliasData, Located, Stmt, StmtKind};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_stmt, unparse_stmt};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct ManualFromImport {
    pub module: String,
    pub name: String,
    pub fixable: bool,
}

impl Violation for ManualFromImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ManualFromImport { module, name, .. } = self;
        format!("Use `from {module} import {name}` in lieu of alias")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|ManualFromImport { module, name, .. }| {
                format!("Replace with `from {module} import {name}`")
            })
    }
}

/// PLR0402
pub fn manual_from_import(checker: &mut Checker, stmt: &Stmt, alias: &Alias, names: &[Alias]) {
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
        ManualFromImport {
            module: module.to_string(),
            name: name.to_string(),
            fixable,
        },
        Range::from(alias),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            unparse_stmt(
                &create_stmt(StmtKind::ImportFrom {
                    module: Some(module.to_string()),
                    names: vec![Located::new(
                        stmt.location,
                        stmt.end_location.unwrap(),
                        AliasData {
                            name: asname.into(),
                            asname: None,
                        },
                    )],
                    level: Some(0),
                }),
                checker.stylist,
            ),
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
