//! Lint rules based on import analysis.
use std::path::Path;

use log::debug;
use rustpython_parser::ast::{StmtKind, Suite};

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};
use ruff_python_ast::types::{Import, Imports};
use ruff_python_ast::visitor::Visitor;

use crate::directives::IsortDirectives;
use crate::registry::Rule;
use crate::rules::isort;
use crate::rules::isort::track::{Block, ImportTracker};
use crate::settings::{flags, Settings};

#[allow(clippy::too_many_arguments)]
pub fn check_imports(
    python_ast: &Suite,
    locator: &Locator,
    indexer: &Indexer,
    directives: &IsortDirectives,
    settings: &Settings,
    stylist: &Stylist,
    autofix: flags::Autofix,
    path: &Path,
    package: Option<&Path>,
) -> (Vec<Diagnostic>, Imports) {
    // Extract all imports from the AST.
    let tracker = {
        let mut tracker = ImportTracker::new(locator, directives, path);
        for stmt in python_ast {
            tracker.visit_stmt(stmt);
        }
        tracker
    };
    let blocks: Vec<&Block> = tracker.iter().collect();

    // Enforce import rules.
    let mut diagnostics = vec![];
    if settings.rules.enabled(Rule::UnsortedImports) {
        for block in &blocks {
            if !block.imports.is_empty() {
                if let Some(diagnostic) = isort::rules::organize_imports(
                    block, locator, stylist, indexer, settings, autofix, package,
                ) {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    if settings.rules.enabled(Rule::MissingRequiredImport) {
        diagnostics.extend(isort::rules::add_required_imports(
            &blocks, python_ast, locator, stylist, settings, autofix,
        ));
    }
    let mut imports = Imports::default();
    if let Some(package) = package {
        let mut imports_vec = vec![];
        let modules: Vec<String> = to_module_path(package, path).unwrap();
        debug!("modules {:?}", modules);
        for &block in &blocks {
            block.imports.iter().for_each(|&stmt| match &stmt.node {
                StmtKind::Import { names } => {
                    imports_vec.extend(names.iter().map(|name| {
                        Import::new(
                            name.node.name.clone(),
                            stmt.location,
                            stmt.end_location.unwrap(),
                        )
                    }));
                }
                StmtKind::ImportFrom {
                    module,
                    names,
                    level,
                } => {
                    // case where module is None with level
                    // case where module isn't None with level
                    // think of more potential relatives
                    let modules = if let Some(module) = module {
                        let level = level.unwrap();
                        if level > 0 {
                            format!("{}.{}.", modules[0..level].join("."), module)
                        } else {
                            format!("{module}.")
                        }
                    } else {
                        // relative import
                        format!(
                            "{}.",
                            modules[..(modules.len() - level.unwrap_or(0))].join(".")
                        )
                    };
                    // let module = if let Some(module) = module { module.clone() } else { "".to_string() };
                    // let modules = modules[..(modules.len()-level.unwrap_or(0))].join(".");
                    // let modules = modules[..level.unwrap_or(0)].join(".");
                    imports_vec.extend(names.iter().map(|name| {
                        Import::new(
                            format!("{}{}", modules, name.node.name),
                            name.location,
                            name.end_location.unwrap(),
                        )
                    }));
                }
                // ImportTracker guarantees that we will only have import statements
                _ => unreachable!("Should only have import statements"),
            });
        }
        let module_path = if let Some(module_path) = to_module_path(package, path) {
            module_path.join(".")
        } else {
            String::new()
        };

        debug!("{module_path} {blocks:#?}");
        debug!("{imports_vec:#?}");

        if !imports_vec.is_empty() {
            imports.insert(&module_path, imports_vec);
            imports.insert_new_module(&module_path, path);
        }
    }

    (diagnostics, imports)
}
