use rustpython_ast::{Located, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn add_check_for_node<T>(xxxxxxxx: &mut xxxxxxxx, node: &Located<T>) {
    let mut check = Diagnostic::new(violations::RewriteCElementTree, Range::from_located(node));
    if xxxxxxxx.patch(check.kind.code()) {
        let contents = xxxxxxxx
            .locator
            .slice_source_code_range(&Range::from_located(node));
        check.amend(Fix::replacement(
            contents.replacen("cElementTree", "ElementTree", 1),
            node.location,
            node.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}

/// UP023
pub fn replace_c_element_tree(xxxxxxxx: &mut xxxxxxxx, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::Import { names } => {
            // Ex) `import xml.etree.cElementTree as ET`
            for name in names {
                if name.node.name == "xml.etree.cElementTree" && name.node.asname.is_some() {
                    add_check_for_node(xxxxxxxx, name);
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
                    add_check_for_node(xxxxxxxx, stmt);
                } else if module == "xml.etree" {
                    // Ex) `from xml.etree import cElementTree as ET`
                    for name in names {
                        if name.node.name == "cElementTree" && name.node.asname.is_some() {
                            add_check_for_node(xxxxxxxx, name);
                        }
                    }
                }
            }
        }
        _ => unreachable!("Expected StmtKind::Import | StmtKind::ImportFrom"),
    }
}
