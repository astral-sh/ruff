use rustpython_ast::{Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// Replaces a given statement with a string
fn update_import(checker: &mut Checker, stmt: &Stmt, new_stmt: String) {
    let mut check = Check::new(CheckKind::RewriteMockImport, Range::from_located(stmt));
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            new_stmt,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}

/// Returns the amount of new lines in a given statement
fn new_line_count(checker: &Checker, stmt: &Stmt) -> usize {
    let contents = checker
        .locator
        .slice_source_code_range(&Range::from_located(stmt));
    let mut count: usize = 0;
    for (_, tok, _) in lexer::make_tokenizer(&contents).flatten() {
        if tok == Tok::Newline {
            count += 1;
        }
    }
    count
}

/// Create the new string for the import
fn create_new_statement(needed_imports: Vec<String>, multi_line: bool) -> String {
    let mut new_stmt = String::new();
    for (i, import) in needed_imports.iter().enumerate() {
        if i != 0 {
            new_stmt.push(',');
        }
        new_stmt.push_str(&format!(" {}", import));
    }
    // We only want to add a new line before if there is an import above
    if !needed_imports.is_empty() {
        new_stmt.insert_str(0, "import");
        new_stmt.push('\n');
    }
    new_stmt.push_str("from unittest import mock\n");
    new_stmt
}

pub fn rewrite_mock_import(checker: &mut Checker, stmt: &Stmt) {
    let mut needed_imports: Vec<String> = vec![];
    let mut needs_updated = false;
    match &stmt.node {
        StmtKind::Import { names } => {
            for item in names {
                let name = &item.node.name;
                if name == "mock" || name == "mock.mock" {
                    needs_updated = true;
                } else {
                    needed_imports.push(name.to_string());
                }
            }
            if needs_updated {
                let new_stmt = create_new_statement(needed_imports, false);
                update_import(checker, stmt, new_stmt);
            }
        },
        StmtKind::ImportFrom { module, names, level } => {
            println!("module: {:?}, names: {:?}, level: {:?}\n", module, names, level);
            let is_multi_line = new_line_count(checker, stmt) > 1;
            println!("=============NL Is Multi: {}", is_multi_line);
        },
        _ => return
    }
}
