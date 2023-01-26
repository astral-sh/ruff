use itertools::Itertools;
use log::error;
use rustpython_ast::{Alias, AliasData, Located};
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::{autofix, violations};

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
    let mut unused_imports: Vec<&Alias> = vec![];
    for alias in names {
        if alias.node.asname.is_some() {
            continue;
        }
        if PY33_PLUS_REMOVE_FUTURES.contains(&alias.node.name.as_str())
            || PY37_PLUS_REMOVE_FUTURES.contains(&alias.node.name.as_str())
        {
            unused_imports.push(alias);
        }
    }

    if unused_imports.is_empty() {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryFutureImport(
            unused_imports
                .iter()
                .map(|alias| alias.node.name.to_string())
                .sorted()
                .collect(),
        ),
        Range::from_located(stmt),
    );

    if checker.patch(diagnostic.kind.rule()) {
        let deleted: Vec<&Stmt> = checker
            .deletions
            .iter()
            .map(std::convert::Into::into)
            .collect();
        let defined_by = checker.current_stmt();
        let defined_in = checker.current_stmt_parent();
        let unused_imports: Vec<String> = unused_imports
            .iter()
            .map(|alias| format!("__future__.{}", alias.node.name))
            .collect();
        match autofix::helpers::remove_unused_imports(
            unused_imports.iter().map(String::as_str),
            defined_by.into(),
            defined_in.map(std::convert::Into::into),
            &deleted,
            checker.locator,
            checker.indexer,
            checker.stylist,
        ) {
            Ok(fix) => {
                if fix.content.is_empty() || fix.content == "pass" {
                    checker.deletions.insert(defined_by.clone());
                }
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to remove `__future__` import: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}
