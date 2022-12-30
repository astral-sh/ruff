use rustpython_ast::{Stmt, StmtKind, Located, AliasData};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::ast::whitespace::indentation;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::isort::helpers::trailing_comma;

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
    println!("stmt: {:?}", contents);
    let mut count: usize = 0;
    // I tried using tokens here, but the count didnt work
    for character in contents.as_ref().chars() {
        if character == '\n' {
            count += 1;
        }
    }
    count
}

/// Create the new string for the import
fn create_new_statement(needed_imports: Vec<String>, beginning: &str, indent: &str, multi_line: bool) -> String {
    let mut new_stmt = String::new();
    // If this is a mulit line import, we need to add the beginning
    if multi_line {
        new_stmt.push_str(" (\n")
    }
    for (i, import) in needed_imports.iter().enumerate() {
        if i != 0 {
            new_stmt.push(',');
            if multi_line {
                new_stmt.push_str("\n");
                new_stmt.push_str(indent);
            }
        }
        new_stmt.push_str(&format!(" {}", import));
    }
    // We only want to add a new line before if there is an import above
    if !needed_imports.is_empty() {
        new_stmt.insert_str(0, beginning);
        new_stmt.push('\n');
        new_stmt.push_str(indent);
    }
    new_stmt.push_str("from unittest import mock\n");
    new_stmt
}

/// Adds needed imports to the given vector, and returns whether a mock was imported
fn filter_names(names: &Vec<Located<AliasData>>) -> (Vec<String>, bool) {
    let mut needed_imports: Vec<String> = vec![];
    let mut needs_updated = false;
    for item in names {
        let name = &item.node.name;
        if name == "mock" || name == "mock.mock" {
            needs_updated = true;
        } else {
            needed_imports.push(name.to_string());
        }
    }
    (needed_imports, needs_updated)
}

pub fn rewrite_mock_import(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::Import { names } => {
            let (needed_imports, needs_updated) = filter_names(names);
            if needs_updated {
                let indent = indentation(checker, stmt);
                let new_stmt = create_new_statement(needed_imports, "import", &indent, false);
                update_import(checker, stmt, new_stmt);
            }
        },
        StmtKind::ImportFrom { module, names, .. } => {
            if let Some(name) = module {
                if name == "mock" {
                    let (needed_imports, needs_updated) = filter_names(names);
                    if needs_updated  {
                        let indent = indentation(checker, stmt);
                        let beginning = format!("from {} import", name);
                        let is_multi_line = new_line_count(checker, stmt) > 1;
                        let new_stmt = create_new_statement(needed_imports, &beginning, &indent, is_multi_line);
                        update_import(checker, stmt, new_stmt);
                    }
                }
            }
        },
        _ => return
    }
}
