use rustpython_ast::{Stmt, StmtKind};
use libcst_native::{Import, ImportFrom, ImportAlias, Name, NameOrAttribute, Expression, CodegenState, Codegen, ImportNames};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::cst::matchers::{match_module, match_import, match_import_from};
use crate::source_code_locator::SourceCodeLocator;

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

struct CleanImport<'a> {
    pub aliases: Vec<ImportAlias<'a>>,
    has_mock: bool
}

impl<'a> CleanImport<'a> {
    fn new(aliases: Vec<ImportAlias<'a>>, has_mock: bool) -> Self {
        Self {aliases, has_mock}
    }
}

impl<'a> Default for CleanImport<'a> {
    fn default() -> Self {
        Self {
            aliases: vec![],
            has_mock: false,
        }
    }
}

fn clean_import_aliases<'a>(aliases: &'a Vec<ImportAlias>) -> CleanImport<'a> {
    let mut new_aliases: Vec<ImportAlias<'_>> = vec![];
    let mut has_mock = false;
    for alias in aliases {
        let ImportAlias { name, .. } = alias;
        match name {
            NameOrAttribute::N(name_struct) => {
                if name_struct.value == "mock" {
                    has_mock = true;
                } else {
                    new_aliases.push(alias.clone());
                }
            },
            NameOrAttribute::A(attribute_struct) => {
                let item = *attribute_struct.clone().value;
                if let Expression::Name(name_struct) = &item {
                    let Name { value, .. } = attribute_struct.attr;
                    if name_struct.value == "mock" && value == "mock" {
                        has_mock = true;
                    } else {
                        new_aliases.push(alias.clone());
                    }
                } else {return CleanImport::default()}

            }
        }
    }
    CleanImport::new(new_aliases, has_mock)
}

fn format_import(locator: &SourceCodeLocator, stmt: &Stmt, indent: &str) -> Option<String> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text).unwrap();
    let mut import = match match_import(&mut tree) {
        Err(_) => return None,
        Ok(import_item) => import_item,
    };
    let Import { names, .. } = import.clone();
    let clean_import = clean_import_aliases(&names);
    if clean_import.has_mock && clean_import.aliases.is_empty() {
        Some(format!("from unittest import mock"))
    } else if clean_import.has_mock {
        import.names = clean_import.aliases;
        let mut state = CodegenState::default();
        tree.codegen(&mut state);
        let mut base_string = state.to_string();
        base_string.push_str(&format!("\n{indent}from unittest import mock"));
        Some(base_string)
    } else {
        None
    }
}

fn format_import_from(locator: &SourceCodeLocator, stmt: &Stmt, indent: &str) -> Option<String> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text).unwrap();
    let mut import = match match_import_from(&mut tree) {
        Err(_) => return None,
        Ok(import_item) => import_item,
    };
    let ImportFrom { names: from_names, .. } = import.clone();
    if let ImportNames::Aliases(names) = from_names {
        let clean_import = clean_import_aliases(&names);
        if clean_import.has_mock && clean_import.aliases.is_empty() {
            return Some(format!("from unittest import mock"));
        } else if clean_import.has_mock {
            import.names = ImportNames::Aliases(clean_import.aliases);
            let mut state = CodegenState::default();
            tree.codegen(&mut state);
            let mut base_string = state.to_string();
            base_string.push_str(&format!("\n{indent}from unittest import mock"));
            return Some(base_string)
        }
    }
    None
}

pub fn rewrite_mock_import(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::Import { .. } => {
            let indent = indentation(checker, stmt);
            match format_import(checker.locator, stmt, &indent) {
                None => return,
                Some(formatted) => update_import(checker, stmt, formatted)
            }
        }
        StmtKind::ImportFrom {module: Some(module), .. } => {
            if module == "mock" {
                let indent = indentation(checker, stmt);
                match format_import_from(checker.locator, stmt, &indent) {
                    None => return,
                    Some(formatted) => update_import(checker, stmt, formatted)
                }
            }
        }
        _ => (),
    }
}
