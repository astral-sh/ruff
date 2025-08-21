use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of the `xml.etree.cElementTree` module.
///
/// ## Why is this bad?
/// In Python 3.3, `xml.etree.cElementTree` was deprecated in favor of
/// `xml.etree.ElementTree`.
///
/// ## Example
/// ```python
/// from xml.etree import cElementTree as ET
/// ```
///
/// Use instead:
/// ```python
/// from xml.etree import ElementTree as ET
/// ```
///
/// ## References
/// - [Python documentation: `xml.etree.ElementTree`](https://docs.python.org/3/library/xml.etree.elementtree.html)
#[derive(ViolationMetadata)]
pub(crate) struct DeprecatedCElementTree;

impl AlwaysFixableViolation for DeprecatedCElementTree {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`cElementTree` is deprecated, use `ElementTree`".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `ElementTree`".to_string()
    }
}

fn add_check_for_node<T>(checker: &Checker, node: &T)
where
    T: Ranged,
{
    let mut diagnostic = checker.report_diagnostic(DeprecatedCElementTree, node.range());
    let contents = checker.locator().slice(node);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        contents.replacen("cElementTree", "ElementTree", 1),
        node.range(),
    )));
}

/// UP023
pub(crate) fn deprecated_c_element_tree(checker: &Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport {
            names,
            range: _,
            node_index: _,
        }) => {
            // Ex) `import xml.etree.cElementTree as ET`
            for name in names {
                if &name.name == "xml.etree.cElementTree" && name.asname.is_some() {
                    add_check_for_node(checker, name);
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom {
            module,
            names,
            level,
            range: _,
            node_index: _,
        }) => {
            if *level > 0 {
                // Ex) `import .xml.etree.cElementTree as ET`
            } else if let Some(module) = module {
                if module == "xml.etree.cElementTree" {
                    // Ex) `from xml.etree.cElementTree import XML`
                    add_check_for_node(checker, stmt);
                } else if module == "xml.etree" {
                    // Ex) `from xml.etree import cElementTree as ET`
                    for name in names {
                        if &name.name == "cElementTree" && name.asname.is_some() {
                            add_check_for_node(checker, name);
                        }
                    }
                }
            }
        }
        _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
    }
}
