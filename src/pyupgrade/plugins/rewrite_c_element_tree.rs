use rustpython_ast::{AliasData, Located, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

fn check_from_import(module: &Option<String>, names: &Vec<Located<AliasData>>) -> bool {
    if module == &Some("xml.etree.cElementTree".to_string()) {
        return true;
    } else if module == &Some("xml.etree".to_string()) {
        let clean_names: Vec<&Located<AliasData>> = names
            .iter()
            .filter(|name| name.node.name == "cElementTree")
            .collect();
        let item = match clean_names.get(0) {
            None => return false,
            Some(thing) => thing,
        };
        if item.node.asname.is_some() {
            return true;
        }
    }
    false
}

/// UP023
pub fn replace_c_element_tree(checker: &mut Checker, stmt: &Stmt) {
    // Please note that for now this is an exact implementation that only does
    // what pyupgrade does. In the future we could check for cElementTree
    // anywhere as any type of stmt or expr, and replace it
    if match &stmt.node {
        StmtKind::Import { names } => {
            let clean_names: Vec<&Located<AliasData>> = names
                .iter()
                .filter(|name| name.node.name == "xml.etree.cElementTree")
                .collect();
            let item = match clean_names.get(0) {
                None => return,
                Some(thing) => thing,
            };
            item.node.asname.is_some()
        }
        StmtKind::ImportFrom { module, names, .. } => check_from_import(module, names),
        _ => false,
    } {
        let range = Some(Range::from_located(stmt));
        let selected_contents = checker
            .locator
            .slice_source_code_range(&range.unwrap())
            .to_string();
        // This is a hacky way to check for a relative import, but the stmt
        // variable is identical for absolute and relative imports
        if selected_contents.contains(".xml") {
            return;
        }
        let mut check = Check::new(CheckKind::RewriteCElementTree, range.unwrap());
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                selected_contents.replace("cElementTree", "ElementTree"),
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        }
        checker.add_check(check);
    }
}
