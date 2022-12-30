use rustpython_ast::{Located, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

fn update_import(checker: &mut Checker, import: &Located<String>) {
    let mut check = Check::new(CheckKind::RewriteCElementTree, Range::from_located(import));
    if checker.patch(check.kind.code()) {
        let contents = checker
            .locator
            .slice_source_code_range(&Range::from_located(import));
        check.amend(Fix::replacement(
            contents.replacen("cElementTree", "ElementTree", 1),
            import.location,
            import.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}

pub fn rewrite_mock_import(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::Import { names } => {
            println!("names: {:?}", names);
        },
        StmtKind::ImportFrom { module, names, level } => {
            println!("module: {:?}, names: {:?}, level: {:?}", module, names, level);
        },
        _ => return
    }
}
