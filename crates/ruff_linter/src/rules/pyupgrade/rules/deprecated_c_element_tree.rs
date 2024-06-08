use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the `xml.etree.cElementTree` module.
///
/// ## Why is this bad?
/// In Python 3.3, `xml.etree.cElementTree` was deprecated in favor of
/// `xml.etree.ElementTree`.
///
/// ## Example
/// ```python
/// from xml.etree import cElementTree
/// ```
///
/// Use instead:
/// ```python
/// from xml.etree import ElementTree
/// ```
///
/// ## References
/// - [Python documentation: `xml.etree.ElementTree`](https://docs.python.org/3/library/xml.etree.elementtree.html)
#[violation]
pub struct DeprecatedCElementTree;

impl AlwaysFixableViolation for DeprecatedCElementTree {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`cElementTree` is deprecated, use `ElementTree`")
    }

    fn fix_title(&self) -> String {
        "Replace with `ElementTree`".to_string()
    }
}

fn add_check_for_node<T>(checker: &mut Checker, node: &T)
where
    T: Ranged,
{
    let mut diagnostic = Diagnostic::new(DeprecatedCElementTree, node.range());
    let contents = checker.locator().slice(node);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        contents.replacen("cElementTree", "ElementTree", 1),
        node.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// UP023
pub(crate) fn deprecated_c_element_tree(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport { names, range: _ }) => {
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
