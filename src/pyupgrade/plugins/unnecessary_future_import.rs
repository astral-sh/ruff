use rustpython_ast::{AliasData, Located};
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::pyupgrade::fixes;
use crate::settings::types::PythonVersion;

pub const PY33_PLUS_REMOVE_FUTURES: &[&str] = &[
    "nested_scopes",
    "generators",
    "with_statement",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
];

pub const PY37_PLUS_REMOVE_FUTURES: &[&str] = &[
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

pub fn unnecessary_future_import(checker: &mut Checker, stmt: &Stmt, names: &[Located<AliasData>]) {
    let target_version = checker.settings.target_version;

    let mut removable_index = vec![];
    let mut removable_names = vec![];
    for (index, alias) in names.iter().enumerate() {
        let name = &alias.node.name.as_str();
        if (target_version >= PythonVersion::Py33 && PY33_PLUS_REMOVE_FUTURES.contains(name))
            || (target_version >= PythonVersion::Py37 && PY37_PLUS_REMOVE_FUTURES.contains(name))
        {
            removable_index.push(index);
            removable_names.push(name.to_string())
        }
    }

    if !removable_names.is_empty() {
        let mut check = Check::new(
            CheckKind::UnnecessaryFutureImports(removable_names),
            Range::from_located(stmt),
        );
        if checker.patch() {
            let deleted: Vec<&Stmt> = checker
                .deletions
                .iter()
                .map(|index| checker.parents[*index])
                .collect();
            if let Ok(fix) = fixes::remove_unnecessary_future_import(
                checker.locator,
                stmt,
                &removable_index,
                &deleted,
            ) {
                check.amend(fix);
            }
        }
        checker.add_check(check);
    }
}
