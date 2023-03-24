//! Lint rules based on import analysis.
use std::path::Path;

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
    let mut imports_vec = vec![];
    // find the difference between the current path and the package root (inc root)
    let modules: Vec<&str> = match package {
        Some(package) => {
            let mut modules: Vec<&str> = vec![package.iter().last().unwrap().to_str().unwrap()];
            modules.extend(
                path.strip_prefix(package)
                    .iter()
                    .rev()
                    // we don't want the end module as it is the current one
                    .skip(1)
                    .map(|p| p.to_str().unwrap())
                    .rev(),
            );
            modules
        }
        None => path
            .iter()
            .rev()
            .skip(1)
            .take(1)
            .map(|p| p.to_str().unwrap())
            .collect::<Vec<_>>(),
    };
    for &block in &blocks {
        block.imports.iter().for_each(|&stmt| match &stmt.node {
            StmtKind::Import { names } => {
                // from testing, seems this should only have one entry
                imports_vec.push(Import::new(
                    names[0].node.name.clone(),
                    stmt.location,
                    stmt.end_location.unwrap(),
                ));
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                imports_vec.extend(names.iter().map(|name| {
                    Import::new(
                        Imports::expand_relative(&modules, module, &name.node.name, level),
                        name.location,
                        name.end_location.unwrap(),
                    )
                }));
            }
            // ImportTracker guarantees that we will only have import statements
            _ => unreachable!("Should only have import statements"),
        });
    }
    let module_path = if let Some(package) = package {
        if let Some(module_path) = to_module_path(package, path) {
            module_path.join(".")
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    if !imports_vec.is_empty() {
        imports.insert(&module_path, imports_vec);
        imports.insert_new_module(&module_path, path);
    }

    (diagnostics, imports)
}
