use rustpython_ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// UP023
pub fn replace_c_element_tree(checker: &mut Checker, stmt: &Stmt) {
    if match &stmt.node {
        StmtKind::Import { names } => {
            // Ex) `import xml.etree.cElementTree as ET`
            if let Some(name) = names
                .iter()
                .find(|name| name.node.name == "xml.etree.cElementTree")
            {
                name.node.asname.is_some()
            } else {
                false
            }
        }
        StmtKind::ImportFrom {
            module,
            names,
            level,
        } => {
            if level.map_or(false, |level| level > 0) {
                // Ex) `import .xml.etree.cElementTree as ET`
                false
            } else if let Some(module) = module {
                if module == "xml.etree.cElementTree" {
                    // Ex) `from xml.etree.cElementTree import XML`
                    true
                } else if module == "xml.etree" {
                    // Ex) `from xml.etree import cElementTree as ET`
                    if let Some(name) = names.iter().find(|name| name.node.name == "cElementTree") {
                        name.node.asname.is_some()
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => unreachable!("Expected StmtKind::Import | StmtKind::ImportFrom"),
    } {
        let mut check = Check::new(CheckKind::RewriteCElementTree, Range::from_located(stmt));
        if checker.patch(check.kind.code()) {
            let contents = checker
                .locator
                .slice_source_code_range(&Range::from_located(stmt));
            check.amend(Fix::replacement(
                contents.replace("cElementTree", "ElementTree"),
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        }
        checker.add_check(check);
    }
}
