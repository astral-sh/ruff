use itertools::Itertools;
use log::error;
use rustc_hash::FxHashSet;
use rustpython_ast::{AliasData, Located};
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::pyupgrade::fixes;
use crate::settings::types::PythonVersion;

const PY33_PLUS_REMOVE_FUTURES: &[&str] = &[
    "nested_scopes",
    "generators",
    "with_statement",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
];

const PY37_PLUS_REMOVE_FUTURES: &[&str] = &[
    "nested_scopes",
    "generators",
    "with_statement",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
    "generator_stop",
];

/// UP010
pub fn unnecessary_future_import(checker: &mut Checker, stmt: &Stmt, names: &[Located<AliasData>]) {
    let target_version = checker.settings.target_version;

    let mut removable_index: Vec<usize> = vec![];
    let mut removable_names: FxHashSet<&str> = FxHashSet::default();
    for (index, alias) in names.iter().enumerate() {
        let name = alias.node.name.as_str();
        if (target_version >= PythonVersion::Py33 && PY33_PLUS_REMOVE_FUTURES.contains(&name))
            || (target_version >= PythonVersion::Py37 && PY37_PLUS_REMOVE_FUTURES.contains(&name))
        {
            removable_index.push(index);
            removable_names.insert(name);
        }
    }

    if removable_index.is_empty() {
        return;
    }
    let mut check = Check::new(
        CheckKind::UnnecessaryFutureImport(
            removable_names
                .into_iter()
                .map(String::from)
                .sorted()
                .collect(),
        ),
        Range::from_located(stmt),
    );

    if checker.patch(check.kind.code()) {
        let deleted: Vec<&Stmt> = checker.deletions.iter().map(|node| node.0).collect();
        let defined_by = checker.current_stmt();
        let defined_in = checker.current_stmt_parent();
        match fixes::remove_unnecessary_future_import(
            checker.locator,
            &removable_index,
            defined_by.0,
            defined_in.map(|node| node.0),
            &deleted,
        ) {
            Ok(fix) => {
                if fix.content.is_empty() || fix.content == "pass" {
                    checker.deletions.insert(defined_by.clone());
                }
                check.amend(fix);
            }
            Err(e) => error!("Failed to remove __future__ import: {e}"),
        }
    }
    checker.add_check(check);
}
