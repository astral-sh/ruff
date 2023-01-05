use itertools::Itertools;
use log::error;
use rustpython_ast::{Alias, AliasData, Located};
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};
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

    let mut unused_imports: Vec<&Alias> = vec![];
    for alias in names {
        if alias.node.asname.is_some() {
            continue;
        }
        if (target_version >= PythonVersion::Py33
            && PY33_PLUS_REMOVE_FUTURES.contains(&alias.node.name.as_str()))
            || (target_version >= PythonVersion::Py37
                && PY37_PLUS_REMOVE_FUTURES.contains(&alias.node.name.as_str()))
        {
            unused_imports.push(alias);
        }
    }

    if unused_imports.is_empty() {
        return;
    }
    let mut check = Check::new(
        CheckKind::UnnecessaryFutureImport(
            unused_imports
                .iter()
                .map(|alias| alias.node.name.to_string())
                .sorted()
                .collect(),
        ),
        Range::from_located(stmt),
    );

    if checker.patch(check.kind.code()) {
        let deleted: Vec<&Stmt> = checker.deletions.iter().map(|node| node.0).collect();
        let defined_by = checker.current_stmt();
        let defined_in = checker.current_stmt_parent();
        let unused_imports: Vec<String> = unused_imports
            .iter()
            .map(|alias| format!("__future__.{}", alias.node.name))
            .collect();
        match autofix::helpers::remove_unused_imports(
            unused_imports.iter().map(std::string::String::as_str),
            defined_by.0,
            defined_in.map(|node| node.0),
            &deleted,
            checker.locator,
        ) {
            Ok(fix) => {
                if fix.content.is_empty() || fix.content == "pass" {
                    checker.deletions.insert(defined_by.clone());
                }
                check.amend(fix);
            }
            Err(e) => error!("Failed to remove `__future__` import: {e}"),
        }
    }
    checker.add_check(check);
}
