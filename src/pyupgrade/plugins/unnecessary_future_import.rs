use std::collections::BTreeSet;

use rustpython_ast::{AliasData, Located};
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::check_ast::Checker;
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

/// U010
pub fn unnecessary_future_import(checker: &mut Checker, stmt: &Stmt, names: &[Located<AliasData>]) {
    let target_version = checker.settings.target_version;

    let mut removable_index: Vec<usize> = vec![];
    let mut removable_names: BTreeSet<&str> = BTreeSet::new();
    for (index, alias) in names.iter().enumerate() {
        let name = alias.node.name.as_str();
        if (target_version >= PythonVersion::Py33 && PY33_PLUS_REMOVE_FUTURES.contains(&name))
            || (target_version >= PythonVersion::Py37 && PY37_PLUS_REMOVE_FUTURES.contains(&name))
        {
            removable_index.push(index);
            removable_names.insert(name);
        }
    }

    if !removable_index.is_empty() {
        let mut check = Check::new(
            CheckKind::UnnecessaryFutureImport(
                removable_names.into_iter().map(String::from).collect(),
            ),
            Range::from_located(stmt),
        );
        if checker.patch() {
            let context = checker.binding_context();
            let deleted: Vec<&Stmt> = checker
                .deletions
                .iter()
                .map(|index| checker.parents[*index])
                .collect();
            if let Ok(fix) = fixes::remove_unnecessary_future_import(
                checker.locator,
                &removable_index,
                checker.parents[context.defined_by],
                context.defined_in.map(|index| checker.parents[index]),
                &deleted,
            ) {
                check.amend(fix);
            }
        }
        checker.add_check(check);
    }
}
