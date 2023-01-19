use rustpython_ast::{Located, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

fn add_check_for_node<T>(checker: &mut Checker, node: &Located<T>) {
    let mut diagnostic =
        Diagnostic::new(violations::RewriteCElementTree, Range::from_located(node));
    if checker.patch(diagnostic.kind.rule()) {
        let contents = checker
            .locator
            .slice_source_code_range(&Range::from_located(node));
        diagnostic.amend(Fix::replacement(
            contents.replacen("cElementTree", "ElementTree", 1),
            node.location,
            node.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// UP023
pub fn replace_c_element_tree(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::Import { names } => {
            // Ex) `import xml.etree.cElementTree as ET`
            for name in names {
                if name.node.name == "xml.etree.cElementTree" && name.node.asname.is_some() {
                    add_check_for_node(checker, name);
                }
            }
        }
        StmtKind::ImportFrom {
            module,
            names,
            level,
        } => {
            if level.map_or(false, |level| level > 0) {
                // Ex) `import .xml.etree.cElementTree as ET`
            } else if let Some(module) = module {
                if module == "xml.etree.cElementTree" {
                    // Ex) `from xml.etree.cElementTree import XML`
                    add_check_for_node(checker, stmt);
                } else if module == "xml.etree" {
                    // Ex) `from xml.etree import cElementTree as ET`
                    for name in names {
                        if name.node.name == "cElementTree" && name.node.asname.is_some() {
                            add_check_for_node(checker, name);
                        }
                    }
                }
            }
        }
        _ => unreachable!("Expected StmtKind::Import | StmtKind::ImportFrom"),
    }
}
