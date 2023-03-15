//! Lint rules based on import analysis.
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;
use rustpython_parser::ast::{StmtKind, Suite};

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};
use ruff_python_ast::types::Import;
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
) -> (Vec<Diagnostic>, FxHashMap<PathBuf, Vec<Import>>) {
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
    let mut imports: FxHashMap<PathBuf, Vec<Import>> = FxHashMap::default();
    let mut imports_vec = vec![];
    for &block in &blocks {
        block.imports.iter().for_each(|&stmt| match &stmt.node {
            StmtKind::Import { names } => {
                // from testing, seems this should only have one entry
                imports_vec.push(Import {
                    name: names[0].node.name.clone(),
                    location: stmt.location,
                    end_location: stmt.end_location.unwrap(),
                });
            }
            StmtKind::ImportFrom { module, names, .. } => imports_vec.extend(
                names
                    .iter()
                    .map(|name| Import {
                        name: format!(
                            "{}.{}",
                            module.as_ref().unwrap_or(&String::new()),
                            name.node.name
                        ),
                        location: name.location,
                        end_location: name.end_location.unwrap(),
                    })
            ),
            _ => unreachable!("Should only have import statements"),
        });
    }

    imports.insert(path.to_owned(), imports_vec);
    (diagnostics, imports)
}
