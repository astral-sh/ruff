//! Lint rules based on import analysis.

use std::path::Path;

use rustpython_parser::ast::{Located, Location, StmtKind, Suite};

use crate::ast::visitor::Visitor;
use crate::directives::IsortDirectives;
use crate::registry::{Diagnostic, Rule};
use crate::rules::isort;
use crate::rules::isort::track::{Block, ImportTracker};
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};

#[derive(Debug, Clone, PartialEq)]
pub struct ImportCheck {
    pub name: String,
    pub location: Location,
}

#[allow(clippy::too_many_arguments)]
pub fn check_imports<'a>(
    python_ast: &'a Suite,
    locator: &'a Locator<'a>,
    indexer: &'a Indexer,
    directives: &'a IsortDirectives,
    settings: &'a Settings,
    stylist: &'a Stylist<'a>,
    autofix: flags::Autofix,
    path: &'a Path,
    package: Option<&'a Path>,
) -> (Vec<Diagnostic>, Vec<Vec<Located<StmtKind>>>) {
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

    let imports_vec: Vec<Vec<Located<StmtKind>>> = blocks
        .iter()
        .map(|&block| block.imports.iter().map(|&stmt| stmt.clone()).collect())
        .collect();
    (diagnostics, imports_vec)
}
