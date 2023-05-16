use rustpython_parser::ast::{self, Attributed, Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct DeprecatedCElementTree;

impl AlwaysAutofixableViolation for DeprecatedCElementTree {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`cElementTree` is deprecated, use `ElementTree`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `ElementTree`".to_string()
    }
}

fn add_check_for_node<T>(checker: &mut Checker, node: &Attributed<T>) {
    let mut diagnostic = Diagnostic::new(DeprecatedCElementTree, node.range());
    if checker.patch(diagnostic.kind.rule()) {
        let contents = checker.locator.slice(node.range());
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            contents.replacen("cElementTree", "ElementTree", 1),
            node.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// UP023
pub(crate) fn deprecated_c_element_tree(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::Import(ast::StmtImport { names }) => {
            // Ex) `import xml.etree.cElementTree as ET`
            for name in names {
                if &name.node.name == "xml.etree.cElementTree" && name.node.asname.is_some() {
                    add_check_for_node(checker, name);
                }
            }
        }
        StmtKind::ImportFrom(ast::StmtImportFrom {
            module,
            names,
            level,
        }) => {
            if level.map_or(false, |level| level.to_u32() > 0) {
                // Ex) `import .xml.etree.cElementTree as ET`
            } else if let Some(module) = module {
                if module == "xml.etree.cElementTree" {
                    // Ex) `from xml.etree.cElementTree import XML`
                    add_check_for_node(checker, stmt);
                } else if module == "xml.etree" {
                    // Ex) `from xml.etree import cElementTree as ET`
                    for name in names {
                        if &name.node.name == "cElementTree" && name.node.asname.is_some() {
                            add_check_for_node(checker, name);
                        }
                    }
                }
            }
        }
        _ => panic!("Expected StmtKind::Import | StmtKind::ImportFrom"),
    }
}
