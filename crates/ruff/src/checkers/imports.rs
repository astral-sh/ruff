//! Lint rules based on import analysis.
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;
use rustpython_parser::ast::{StmtKind, Suite};

use crate::ast::types::Import;
use crate::ast::visitor::Visitor;
use crate::directives::IsortDirectives;
use crate::registry::{Diagnostic, Rule};
use crate::rules::isort;
use crate::rules::isort::track::{Block, ImportTracker};
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};

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
) -> (Vec<Diagnostic>, FxHashMap<Option<PathBuf>, Vec<Import>>) {
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
    if settings.rules.enabled(&Rule::UnsortedImports) {
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
    if settings.rules.enabled(&Rule::MissingRequiredImport) {
        diagnostics.extend(isort::rules::add_required_imports(
            &blocks, python_ast, locator, stylist, settings, autofix,
        ));
    }
    let mut imports: FxHashMap<Option<PathBuf>, Vec<Import>> = FxHashMap::default();
    let mut imports_vec = vec![];
    for &block in blocks.iter() {
        block.imports.iter().for_each(|&stmt| match &stmt.node {
            StmtKind::Import { names } => {
                // from testing, seems this should only have one entry
                imports_vec.push(Import {
                    name: names[0].node.name.to_owned(),
                    location: stmt.location,
                    end_location: stmt.end_location.unwrap(),
                });
            }
            StmtKind::ImportFrom { module, names, .. } => imports_vec.extend(
                names
                    .iter()
                    .map(|name| Import {
                        name: format!("{}{}", { if let Some(n) = module {
                            n } else { "" }}, name.node.name),
                        location: name.location,
                        end_location: name.end_location.unwrap(),
                    })
                    .collect::<Vec<Import>>(),
            ),
            _ => unreachable!("Should only have import statements"),
        });
    }

    // to avoid depedence on ref to python_ast
    let package = if let Some(package_path) = package {
        Some(package_path.to_path_buf())
    } else {
        None
    };

    imports.insert(package, imports_vec);
    println!("imports.rs {imports:?}");
    (diagnostics, imports)
}
