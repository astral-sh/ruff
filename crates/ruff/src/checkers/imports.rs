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
    if let Some(package) = package {
        let mut imports_vec = vec![];
        let modules_vec: Vec<String> = to_module_path(package, path).unwrap_or_default();
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
                    let modules = if let Some(module) = module {
                        match level.unwrap() {
                            0 => format!("{module}."),
                            l => format!(
                                "{}.{}.",
                                modules_vec[..(modules_vec.len() - l)].join("."),
                                module
                            ),
                        }
                    } else {
                        // relative import only
                        format!(
                            "{}.",
                            modules_vec[..(modules_vec.len() - level.unwrap_or(0))].join(".")
                        )
                    };
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
        let module_path = modules_vec.join(".");

        if !imports_vec.is_empty() {
            imports.insert(&module_path, imports_vec);
            imports.insert_new_module(&module_path, path);
        }
    }

    (diagnostics, imports)
}
