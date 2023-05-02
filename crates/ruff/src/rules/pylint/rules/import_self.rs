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
        format!("Module imports itself")
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

fn check_import_self(stmt: &Stmt, path: &Path, module_path: Option<&[String]>) -> bool {
    match &stmt.node {
        StmtKind::Import { names, .. } => {
            if let Some(module_path) = module_path {
                names
                    .iter()
                    .any(|name| name.node.name.split('.').eq(module_path))
            } else if let Some(module) = get_module_from_path(path) {
                names.iter().any(|name| name.node.name == module)
            } else {
                false
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
                    module.split('.').eq(module_path.iter())
                } else {
                    if *level == Some(1) && module_path.last().is_some() {
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
    module_path: Option<&[String]>,
    locator: &Locator,
) -> Option<Diagnostic> {
    if check_import_self(stmt, path, module_path) {
        Some(Diagnostic::new(ImportSelf, identifier_range(stmt, locator)))
    } else {
        None
    }
}
