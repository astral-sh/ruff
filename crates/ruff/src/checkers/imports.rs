//! Lint rules based on import analysis.

use std::path::Path;

use rustpython_parser::ast::{Location, StmtKind, Suite};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::directives::IsortDirectives;
use crate::registry::{Diagnostic, Rule};
use crate::rules::isort::{self, comments};
use crate::rules::isort::track::{Block, ImportTracker};
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};


#[derive(Debug, Clone, PartialEq)]
pub struct ImportCheck {
    pub name: String,
    pub location: Location
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
) -> Vec<Diagnostic> {
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
    // located imports?
    // line and col info for whole import line
    // statement node, ImportFrom or Import
    // location start/end of the import data
    // we need to infer relative import roots
    // don't bother with third-party stuff
    let annotated_imports = &blocks
        .iter()
        .map(|&block| {
            // Extract comments. Take care to grab any inline comments from the last line.
            let comments = comments::collect_comments(
                &Range::new(
                    range.location,
                    Location::new(range.end_location.row() + 1, 0),
                ),
                locator,
            );
        }).collect();
    isort::annotate::annotate_imports()
    let mut imports = vec![];
    imports.extend(
        blocks
            .iter()
            .map(|&block|
                block.imports.iter()
                .filter(|&import|
                    matches!(import.node, StmtKind::Import | StmtKind::ImportFrom))
                .map(|&located| {
                    match &located {
                        StmtKind::Import => {
                            // maybe just return located node here or transform into
                            // something we'd use only?
                            todo!();
                        },
                        StmtKind::ImportFrom => {
                            // am thinking that we'd transform this into a fully-qualified
                            // import path?
                            todo!();
                        }
                    }
                })
            )
    );

    let imports_vec = blocks
        .iter()
        .map(|block|
            block.imports
                .iter()
                .map(|located_stmt| (*located_stmt).clone())
                .collect::<Vec<_>>()
        ).collect::<Vec<_>>();
        println!("{:#?}", imports_vec);
    diagnostics
}
