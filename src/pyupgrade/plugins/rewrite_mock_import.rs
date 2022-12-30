use rustpython_ast::{AliasData, Located, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::isort::helpers::trailing_comma;
use crate::isort::types::TrailingComma;

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
    // I tried using tokens here, but the count didn't work
    for character in contents.as_ref().chars() {
        if character == '\n' {
            count += 1;
        }
    }
    count
}

/// Create the new string for the import
fn create_new_statement(
    needed_imports: Vec<String>,
    beginning: &str,
    indent: &str,
    multi_line: bool,
    magic_comma: bool,
) -> String {
    let mut new_stmt = String::new();
    // If this is a mulit line import, we need to add the beginning
    if multi_line {
        new_stmt.push_str(" (")
    }
    for (i, import) in needed_imports.iter().enumerate() {
        // We NEVER want a comma before the first item (since this would be import,
        // example)
        if i != 0 {
            new_stmt.push(',');
        }
        // If this is a multi line import, we need to go to the next line and add an
        // import
        if multi_line {
            new_stmt.push('\n');
            new_stmt.push_str(indent);
        }
        // If this is multiline we will need an additonaly indent of 4 spaces beyond the
        // `indent`
        let gap = if multi_line { "    " } else { " " };
        new_stmt.push_str(&format!("{gap}{import}"));
    }
    // If the multi-line import had a trailing comma, we need to add it to the last
    // item
    if magic_comma {
        new_stmt.push(',');
    }
    // We only want to add a new line before if there is an import above
    if !needed_imports.is_empty() {
        // We need to add the `beginning` ('import' or 'from example import')
        // We also need to add the correct indent for `from unittest import mock`
        new_stmt.insert_str(0, beginning);
        new_stmt.push('\n');
        new_stmt.push_str(indent);
        // If it is a multi-line statement we also need to handle the closing
        // parenthesis
        if multi_line {
            new_stmt.push(')');
            new_stmt.push('\n');
            new_stmt.push_str(indent);
        }
    }
    new_stmt.push_str("from unittest import mock");
    new_stmt
}

/// Adds needed imports to the given vector, and returns whether a mock was
/// imported
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
                let new_stmt =
                    create_new_statement(needed_imports, "import", &indent, false, false);
                update_import(checker, stmt, new_stmt);
            }
        }
        StmtKind::ImportFrom {
            module: Some(module),
            names,
            ..
        } => {
            if module == "mock" {
                let (needed_imports, needs_updated) = filter_names(names);
                if needs_updated {
                    let indent = indentation(checker, stmt);
                    let beginning = format!("from {module} import");
                    let is_multi_line = new_line_count(checker, stmt) > 1;
                    let mut has_magic_comma: bool = false;
                    // We only need to check for magic commas if it is a multi-line import
                    if is_multi_line {
                        has_magic_comma =
                            trailing_comma(stmt, checker.locator) == TrailingComma::Present;
                    }
                    let new_stmt = create_new_statement(
                        needed_imports,
                        &beginning,
                        &indent,
                        is_multi_line,
                        has_magic_comma,
                    );
                    update_import(checker, stmt, new_stmt);
                }
            }
        }
        _ => (),
    }
}
