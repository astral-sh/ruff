use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;
use rustpython_parser::ast::{Stmt, StmtKind};

#[violation]
pub struct ImportSelf;

impl Violation for ImportSelf {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Module import itself")
    }
}

fn get_module_from_path(path: &Path) -> Option<String> {
    Some(
        path.iter()
            .last()?
            .to_str()?
            .strip_suffix(".py")?
            .to_string(),
    )
}

fn check_import_self(stmt: &Stmt, path: &Path, module_path: Option<&Vec<String>>) -> bool {
    match &stmt.node {
        StmtKind::Import { names, .. } => {
            if let Some(module_path) = module_path {
                names
                    .iter()
                    .any(|name| name.node.name == module_path.join("."))
            } else {
                let module = get_module_from_path(path);
                names
                    .iter()
                    .any(|name| Some(&name.node.name) == module.as_ref())
            }
        }
        StmtKind::ImportFrom {
            module,
            level,
            names,
            ..
        } => {
            if let Some(module_path) = module_path {
                if let Some(module) = module {
                    module == &module_path.join(".")
                } else {
                    if matches!(level, Some(1)) {
                        names
                            .iter()
                            .any(|name| Some(&name.node.name) == module_path.last())
                    } else {
                        false
                    }
                }
            } else {
                get_module_from_path(path) == *module
            }
        }
        _ => panic!("import_self supplied with something other than Import|ImportFrom"),
    }
}

/// PLW0406
pub fn import_self(
    stmt: &Stmt,
    path: &Path,
    module_path: Option<&Vec<String>>,
    locator: &Locator,
) -> Option<Diagnostic> {
    if check_import_self(stmt, path, module_path) {
        Some(Diagnostic::new(ImportSelf, identifier_range(stmt, locator)))
    } else {
        None
    }
}
