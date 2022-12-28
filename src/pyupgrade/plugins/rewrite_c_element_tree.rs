use rustpython_ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// UP023
pub fn replace_c_element_tree(checker: &mut Checker, stmt: &Stmt) {
    // Please note that for now this is an exact implementation that only does
    // what pyupgrade does. In the future we could check for cElementTree
    // anywhere as any type of stmt or expr, and replace it
    let mut selected_contents: Option<String> = None;
    let mut range: Option<Range> = None;
    match &stmt.node {
        StmtKind::Import { names } => {
            let item = names.get(0).unwrap();
            if item.node.name == "xml.etree.cElementTree" && item.node.asname.is_some() {
                range = Some(Range::from_located(stmt));
                selected_contents = Some(checker.locator.slice_source_code_range(&range.unwrap()).to_string());
            }
        },
        StmtKind::ImportFrom { module, .. } => {
            if module == &Some("xml.etree.cElementTree".to_string()) {
                range = Some(Range::from_located(stmt));
                selected_contents = Some(checker.locator.slice_source_code_range(&range.unwrap()).to_string());
            }
        },
        _ => (),
    }
    if let Some(selection) = selected_contents {
        let mut check = Check::new(CheckKind::RewriteCElementTree, range.unwrap());
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                selection.replace("cElementTree", "ElementTree"),
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        }
        checker.add_check(check);
    }
}
